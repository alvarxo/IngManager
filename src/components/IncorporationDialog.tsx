import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, FileText, Link2, Plus } from "lucide-react";
import { api } from "@/lib/api";
import type {
  IncorporationGroupDecision,
  IncorporationGroupProposal,
  IncorporationMode,
  IncorporationPreview,
  PhysicalOp,
} from "@/lib/types";
import { formatBytes } from "@/lib/utils";
import { Button } from "./ui/button";

interface Props {
  projectId: string;
  categoryKey: string;
  absolutePaths: string[];
  /** Destination folder when paths may be outside the project (OS drop). */
  destAbsolutePath?: string | null;
  /** Prefer copy when bringing files from outside. */
  preferMove?: boolean;
  /** Skip physical copy when files are already inside the project. */
  alreadyInProject?: boolean;
  onClose: () => void;
  onCompleted: (message: string | null, warning: string | null) => void;
}

const MODE_LABELS: Record<IncorporationMode, string> = {
  register_new: "Registrar documento nuevo",
  associate_existing: "Asociar a documento existente",
  physical_only: "Solo físico (sin lógico)",
};

export function IncorporationDialog({
  projectId,
  categoryKey,
  absolutePaths,
  destAbsolutePath = null,
  preferMove = false,
  alreadyInProject = false,
  onClose,
  onCompleted,
}: Props) {
  const [preview, setPreview] = useState<IncorporationPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [physicalOp, setPhysicalOp] = useState<PhysicalOp>(
    alreadyInProject ? "none" : preferMove ? "move" : "copy",
  );
  const [decisions, setDecisions] = useState<
    Record<string, IncorporationGroupDecision>
  >({});

  useEffect(() => {
    let cancelled = false;
    setPreview(null);
    setError(null);
    void api
      .previewDocumentIncorporation({
        project_id: projectId,
        category_key: categoryKey,
        absolute_paths: absolutePaths,
        dest_absolute_path: destAbsolutePath,
        physical_op: physicalOp,
      })
      .then((p) => {
        if (cancelled) return;
        setPreview(p);
        const next: Record<string, IncorporationGroupDecision> = {};
        for (const g of p.groups) {
          next[g.group_key] = {
            group_key: g.group_key,
            mode: g.suggested_mode,
            document_id: g.suggested_document_id,
            title: g.title,
            assign_code: categoryKey !== "antecedentes",
          };
        }
        setDecisions(next);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [
    projectId,
    categoryKey,
    absolutePaths,
    destAbsolutePath,
    physicalOp,
  ]);

  const summary = useMemo(() => {
    const vals = Object.values(decisions);
    return {
      register: vals.filter((d) => d.mode === "register_new").length,
      associate: vals.filter((d) => d.mode === "associate_existing").length,
      physical: vals.filter((d) => d.mode === "physical_only").length,
    };
  }, [decisions]);

  function updateGroup(
    group: IncorporationGroupProposal,
    patch: Partial<IncorporationGroupDecision>,
  ) {
    setDecisions((prev) => ({
      ...prev,
      [group.group_key]: {
        ...(prev[group.group_key] ?? {
          group_key: group.group_key,
          mode: group.suggested_mode,
          document_id: group.suggested_document_id,
          title: group.title,
          assign_code: true,
        }),
        ...patch,
      },
    }));
  }

  async function confirm() {
    if (!preview) return;
    setBusy(true);
    setError(null);
    try {
      const result = await api.applyDocumentIncorporation({
        project_id: projectId,
        category_key: categoryKey,
        absolute_paths: absolutePaths,
        dest_absolute_path: destAbsolutePath,
        physical_op: physicalOp,
        decisions: Object.values(decisions),
      });
      const parts = [
        result.created_documents
          ? `${result.created_documents} documento(s) nuevo(s)`
          : null,
        result.associated_documents
          ? `${result.associated_documents} asociado(s)`
          : null,
        result.physical_only
          ? `${result.physical_only} solo físico`
          : null,
      ].filter(Boolean);
      onCompleted(
        parts.length ? `Incorporación: ${parts.join(", ")}` : "Incorporación lista",
        result.warning,
      );
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50 p-4">
      <div className="flex max-h-[88vh] w-full max-w-[640px] flex-col border border-border bg-panel shadow-xl">
        <header className="border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold text-text">Incorporar</h2>
          <p className="mt-0.5 text-[12px] text-muted">
            Unifica archivo físico y documento lógico · por defecto registrar +
            vincular (sin copiar a snapshots)
          </p>
        </header>

        <div className="min-h-0 flex-1 overflow-auto px-4 py-3">
          {error && <div className="mb-2 text-[12px] text-error">{error}</div>}
          {!preview && !error && (
            <div className="text-[12px] text-muted">Analizando…</div>
          )}
          {preview?.warning && (
            <div className="mb-3 flex gap-2 border border-warn/40 bg-elevated px-3 py-2 text-[12px] text-warn">
              <AlertTriangle size={14} className="mt-0.5 shrink-0" />
              <span>{preview.warning}</span>
            </div>
          )}

          {!alreadyInProject && (
            <label className="mb-3 flex items-center gap-2 text-[12px] text-muted">
              Operación física (si vienen de fuera)
              <select
                className="h-[28px] border border-border bg-elevated px-2 text-text"
                value={physicalOp}
                onChange={(e) => setPhysicalOp(e.target.value as PhysicalOp)}
              >
                <option value="copy">Copiar</option>
                <option value="move">Mover</option>
                <option value="none">Ninguna</option>
              </select>
            </label>
          )}

          {preview?.groups.map((g) => {
            const d = decisions[g.group_key];
            return (
              <div
                key={g.group_key}
                className="mb-3 border border-border/70 bg-elevated/40 px-3 py-2"
              >
                <div className="mb-1 flex items-center gap-2 text-[13px] font-medium">
                  <FileText size={14} className="text-accent" />
                  {d?.title ?? g.title}
                </div>
                {g.notes && (
                  <div className="mb-1 text-[11px] text-muted">{g.notes}</div>
                )}
                <ul className="mb-2 space-y-0.5">
                  {g.files.map((f) => (
                    <li
                      key={f.absolute_path}
                      className="flex justify-between gap-2 text-[12px]"
                    >
                      <span className="truncate">
                        {f.file_name}{" "}
                        <span className="text-muted">[{f.role}]</span>
                        {f.already_linked_code && (
                          <span className="ml-1 text-accent">
                            → {f.already_linked_code}
                          </span>
                        )}
                      </span>
                      <span className="shrink-0 text-muted">
                        {formatBytes(f.size_bytes)}
                      </span>
                    </li>
                  ))}
                </ul>
                <div className="flex flex-wrap items-center gap-2">
                  <select
                    className="h-[28px] border border-border bg-panel px-2 text-[12px]"
                    value={d?.mode ?? g.suggested_mode}
                    onChange={(e) =>
                      updateGroup(g, {
                        mode: e.target.value as IncorporationMode,
                      })
                    }
                  >
                    {(Object.keys(MODE_LABELS) as IncorporationMode[]).map(
                      (m) => (
                        <option key={m} value={m}>
                          {MODE_LABELS[m]}
                        </option>
                      ),
                    )}
                  </select>
                  {(d?.mode ?? g.suggested_mode) === "associate_existing" && (
                    <select
                      className="h-[28px] min-w-[200px] flex-1 border border-border bg-panel px-2 text-[12px]"
                      value={d?.document_id ?? ""}
                      onChange={(e) =>
                        updateGroup(g, {
                          document_id: e.target.value || null,
                        })
                      }
                    >
                      <option value="">Seleccionar documento…</option>
                      {g.matches.map((m) => (
                        <option key={m.document_id} value={m.document_id}>
                          {m.business_code ?? "—"} · {m.title} (
                          {m.file_count} arch.)
                        </option>
                      ))}
                    </select>
                  )}
                  {(d?.mode ?? g.suggested_mode) === "register_new" && (
                    <input
                      className="h-[28px] min-w-[160px] flex-1 border border-border bg-panel px-2 text-[12px]"
                      value={d?.title ?? g.title}
                      onChange={(e) => updateGroup(g, { title: e.target.value })}
                      placeholder="Título"
                    />
                  )}
                </div>
              </div>
            );
          })}
        </div>

        <footer className="flex flex-wrap items-center gap-2 border-t border-border px-4 py-3">
          <div className="mr-auto flex items-center gap-3 text-[11px] text-muted">
            <span className="inline-flex items-center gap-1">
              <Plus size={12} /> {summary.register} nuevos
            </span>
            <span className="inline-flex items-center gap-1">
              <Link2 size={12} /> {summary.associate} asociar
            </span>
            <span>{summary.physical} solo físico</span>
          </div>
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
            Cancelar
          </Button>
          <Button
            size="sm"
            onClick={() => void confirm()}
            disabled={busy || !preview}
          >
            {busy ? "Incorporando…" : "Incorporar"}
          </Button>
        </footer>
      </div>
    </div>
  );
}
