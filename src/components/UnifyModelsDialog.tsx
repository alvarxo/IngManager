import { useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import type { DocumentRow, UnifyModelsPreview } from "@/lib/types";
import { Button } from "./ui/button";

interface Props {
  projectId: string;
  documents: DocumentRow[];
  initialPrincipalId: string | null;
  suggestedDuplicateIds?: string[];
  onClose: () => void;
  onCompleted: (message: string) => void;
}

function label(doc: { business_code?: string | null; title: string }) {
  return doc.business_code?.trim() || doc.title;
}

export function UnifyModelsDialog({
  projectId,
  documents,
  initialPrincipalId,
  suggestedDuplicateIds = [],
  onClose,
  onCompleted,
}: Props) {
  const [principalId, setPrincipalId] = useState(
    initialPrincipalId ?? documents[0]?.id ?? "",
  );
  const [selectedDups, setSelectedDups] = useState<Record<string, boolean>>(
    () => {
      const init: Record<string, boolean> = {};
      for (const id of suggestedDuplicateIds) {
        if (id !== (initialPrincipalId ?? documents[0]?.id)) init[id] = true;
      }
      return init;
    },
  );
  const [preview, setPreview] = useState<UnifyModelsPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const dupIds = useMemo(
    () => Object.entries(selectedDups).filter(([, v]) => v).map(([k]) => k),
    [selectedDups],
  );

  const candidates = useMemo(
    () => documents.filter((d) => d.id !== principalId),
    [documents, principalId],
  );

  useEffect(() => {
    setSelectedDups((prev) => {
      const next = { ...prev };
      delete next[principalId];
      return next;
    });
  }, [principalId]);

  useEffect(() => {
    let cancelled = false;
    setPreview(null);
    setError(null);
    if (!principalId || dupIds.length === 0) return;
    setBusy(true);
    void api
      .previewUnifyModels({
        project_id: projectId,
        principal_document_id: principalId,
        duplicate_document_ids: dupIds,
        category_key: "modelos",
      })
      .then((p) => {
        if (!cancelled) setPreview(p);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, principalId, dupIds.join("|")]);

  async function apply() {
    if (!principalId || dupIds.length === 0) return;
    if (
      !window.confirm(
        "¿Unificar modelos?\n\nSe crea un backup de la base de datos. Los archivos físicos (.sdb) NO se borran; solo se consolidan registros lógicos.",
      )
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const r = await api.applyUnifyModels({
        project_id: projectId,
        principal_document_id: principalId,
        duplicate_document_ids: dupIds,
        category_key: "modelos",
      });
      onCompleted(
        `Unificados ${r.duplicates_deleted.length} duplicado(s) → ${r.versions_on_principal} versión(es). Backup: ${r.db_backup_path}`,
      );
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4">
      <div className="flex max-h-[90vh] w-full max-w-2xl flex-col border border-border bg-panel shadow-xl">
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold">Unificar modelos…</h2>
          <button
            type="button"
            className="text-muted hover:text-text"
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <div className="min-h-0 flex-1 space-y-3 overflow-auto px-4 py-3 text-[12.5px]">
          <p className="text-muted">
            Conserva UUID, código y título del principal. Transfiere versiones,
            archivos, planillas y relaciones; remapea _vNN → v01… Sin borrar
            archivos físicos.
          </p>

          <label className="block">
            <span className="mb-1 block text-[11px] uppercase text-muted">
              Documento principal
            </span>
            <select
              className="w-full border border-border bg-elevated px-2 py-1.5"
              value={principalId}
              onChange={(e) => setPrincipalId(e.target.value)}
            >
              {documents.map((d) => (
                <option key={d.id} value={d.id}>
                  {label(d)}
                </option>
              ))}
            </select>
          </label>

          <div>
            <div className="mb-1 text-[11px] uppercase text-muted">
              Duplicados a fusionar
            </div>
            {candidates.length === 0 ? (
              <div className="text-muted">No hay otros documentos Modelos.</div>
            ) : (
              <ul className="max-h-40 space-y-1 overflow-auto border border-border bg-elevated p-2">
                {candidates.map((d) => (
                  <li key={d.id} className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      checked={!!selectedDups[d.id]}
                      onChange={(e) =>
                        setSelectedDups((prev) => ({
                          ...prev,
                          [d.id]: e.target.checked,
                        }))
                      }
                    />
                    <span>
                      {label(d)}
                      <span className="ml-1 text-muted">
                        (v{d.current_version_label ?? d.current_version_number ?? "—"} ·{" "}
                        {d.file_count} arch.)
                      </span>
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>

          {error && (
            <div className="border border-error/40 bg-error/10 px-3 py-2 text-error">
              {error}
            </div>
          )}

          {preview && (
            <div className="space-y-2 border border-border bg-elevated p-3">
              <div className="font-medium">Vista previa</div>
              {preview.group_key && (
                <div className="text-muted">Familia: {preview.group_key}</div>
              )}
              <div>
                Rel. a reasignar: {preview.relations_to_rewire} · Planillas:{" "}
                {preview.sheets_to_transfer}
              </div>
              <table className="w-full border-collapse">
                <thead>
                  <tr className="text-left text-muted">
                    <th className="py-0.5 pr-2">Nuevo</th>
                    <th className="py-0.5 pr-2">Hint</th>
                    <th className="py-0.5">Archivo</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.planned_versions.map((v) => (
                    <tr key={`${v.source_version_id}-${v.relative_path}`}>
                      <td className="py-0.5 pr-2">{v.new_label}</td>
                      <td className="py-0.5 pr-2">
                        {v.version_hint ?? "—"}
                      </td>
                      <td className="py-0.5">{v.file_name}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {preview.warnings.length > 0 && (
                <ul className="list-disc pl-4 text-amber-700 dark:text-amber-400">
                  {preview.warnings.map((w) => (
                    <li key={w}>{w}</li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={onClose} disabled={busy}>
            Cancelar
          </Button>
          <Button
            size="sm"
            disabled={busy || !principalId || dupIds.length === 0 || !preview}
            onClick={() => void apply()}
          >
            {busy ? "Procesando…" : "Unificar"}
          </Button>
        </div>
      </div>
    </div>
  );
}
