import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, FolderTree, Play, ScanSearch, SkipForward } from "lucide-react";
import { api } from "@/lib/api";
import type { OrganizerPlanResult, OrganizerScanItem, Project } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type Stage = 1 | 2 | 3 | 4 | 5;

const STAGE_LABELS: Record<Stage, string> = {
  1: "Proyecto",
  2: "Escanear",
  3: "Clasificar",
  4: "Editar plan",
  5: "Aplicar",
};

export function OrganizerView() {
  const qc = useQueryClient();
  const [stage, setStage] = useState<Stage>(1);
  const [scope, setScope] = useState<string>("");
  const [mode, setMode] = useState<"virtual" | "physical">("virtual");
  const [selectedProjectId, setSelectedProjectId] = useState<string>("");
  const [scan, setScan] = useState<OrganizerScanItem[] | null>(null);
  const [plan, setPlan] = useState<OrganizerPlanResult | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const status = useQuery({
    queryKey: ["app-status"],
    queryFn: () => api.getAppStatus(),
  });

  const projects = useQuery({
    queryKey: ["projects", false],
    queryFn: () => api.listProjects(false),
  });

  const roots = status.data?.authorized_roots ?? [];
  const defaultScope =
    scope ||
    roots.find((r) => r.is_primary)?.absolute_path ||
    roots[0]?.absolute_path ||
    "";

  const projectList = projects.data ?? [];
  const selectedProject: Project | undefined = projectList.find(
    (p) => p.id === selectedProjectId,
  );

  // Default single project: first active when list loads
  useEffect(() => {
    if (!selectedProjectId && projectList.length > 0) {
      setSelectedProjectId(projectList[0].id);
    }
  }, [projectList, selectedProjectId]);

  const scopeForProject = useMemo(() => {
    if (!selectedProject) return defaultScope;
    const root = roots.find((r) => r.id === selectedProject.authorized_root_id);
    if (!root) return defaultScope;
    const base = root.absolute_path.replace(/[\\/]+$/, "");
    const rel = selectedProject.root_relative_path.replace(/^[\\/]+/, "");
    return `${base}/${rel}`.replace(/\\/g, "/");
  }, [selectedProject, roots, defaultScope]);

  const doScan = useMutation({
    mutationFn: (path: string) => api.organizerScan(path),
    onSuccess: (data) => {
      setScan(data);
      setPlan(null);
      setMessage(`Clasificados ${data.length} proyectos/carpetas`);
      setStage(3);
    },
    onError: (e) => setMessage(String(e)),
  });

  const doPlan = useMutation({
    mutationFn: () =>
      api.organizerBuildPlan(
        scopeForProject || defaultScope,
        mode,
        selectedProjectId ? [selectedProjectId] : null,
      ),
    onSuccess: (data) => {
      setPlan(data);
      setMessage(`Plan ${data.session.mode}: ${data.items.length} acciones`);
      setStage(4);
    },
    onError: (e) => setMessage(String(e)),
  });

  const doApply = useMutation({
    mutationFn: (sessionId: string) => api.organizerApplyPlan(sessionId),
    onSuccess: (r) => {
      void qc.invalidateQueries({ queryKey: ["projects"] });
      void qc.invalidateQueries({ queryKey: ["categories"] });
      setMessage(
        `Aplicado: ${r.applied} ok · ${r.failed} fallos · ${r.skipped} omitidos` +
          (r.backup_path ? ` · backup: ${r.backup_path}` : ""),
      );
      if (plan) {
        void api.organizerGetPlan(plan.session.id).then(setPlan);
      }
      setStage(5);
    },
    onError: (e) => setMessage(String(e)),
  });

  async function skipItem(itemId: string) {
    if (!plan) return;
    await api.organizerUpdatePlanItem({
      item_id: itemId,
      action: "skip",
      status: "skipped",
    });
    const refreshed = await api.organizerGetPlan(plan.session.id);
    setPlan(refreshed);
  }

  async function restoreItem(itemId: string, action: string) {
    if (!plan) return;
    await api.organizerUpdatePlanItem({
      item_id: itemId,
      action,
      status: "pending",
    });
    const refreshed = await api.organizerGetPlan(plan.session.id);
    setPlan(refreshed);
  }

  return (
    <div className="flex h-full flex-col bg-bg">
      <header className="flex items-center justify-between border-b border-border px-4 py-3">
        <div>
          <h1 className="text-[15px] font-semibold text-text">
            Organizador masivo
          </h1>
          <p className="text-[12px] text-muted">
            5 etapas · proyecto único por defecto · plan editable
          </p>
        </div>
        <FolderTree size={18} className="text-muted" />
      </header>

      <nav className="flex gap-1 border-b border-border px-4 py-2">
        {([1, 2, 3, 4, 5] as Stage[]).map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => setStage(s)}
            className={cn(
              "h-[32px] rounded-[4px] px-2.5 text-[12px]",
              stage === s
                ? "bg-selection text-text"
                : "text-muted hover:bg-elevated hover:text-text",
            )}
          >
            {s}. {STAGE_LABELS[s]}
          </button>
        ))}
      </nav>

      <div className="flex flex-wrap items-end gap-3 border-b border-border px-4 py-3">
        <label className="flex min-w-[220px] flex-1 flex-col gap-1 text-[12px] text-muted">
          Proyecto (por defecto uno)
          <select
            className="h-[34px] rounded-[var(--radius-sm)] border border-border bg-elevated px-2 text-[13px] text-text"
            value={selectedProjectId}
            onChange={(e) => {
              setSelectedProjectId(e.target.value);
              setScan(null);
              setPlan(null);
              setStage(1);
            }}
          >
            {projectList.map((p) => (
              <option key={p.id} value={p.id}>
                {p.code ?? "—"} · {p.name}
              </option>
            ))}
          </select>
        </label>
        <label className="flex min-w-[200px] flex-col gap-1 text-[12px] text-muted">
          Alcance
          <select
            className="h-[34px] rounded-[var(--radius-sm)] border border-border bg-elevated px-2 text-[13px] text-text"
            value={scope || scopeForProject}
            onChange={(e) => setScope(e.target.value)}
          >
            {selectedProject && (
              <option value={scopeForProject}>
                Solo proyecto ({selectedProject.root_relative_path})
              </option>
            )}
            {roots.map((r) => (
              <option key={r.id} value={r.absolute_path}>
                Raíz: {r.label || r.absolute_path}
                {r.is_primary ? " (principal)" : ""}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-muted">
          Modo
          <select
            className="h-[34px] rounded-[var(--radius-sm)] border border-border bg-elevated px-2 text-[13px] text-text"
            value={mode}
            onChange={(e) =>
              setMode(e.target.value as "virtual" | "physical")
            }
          >
            <option value="virtual">Virtual (mapeos SQLite)</option>
            <option value="physical">Físico (rename + backup)</option>
          </select>
        </label>
        <Button
          type="button"
          variant="secondary"
          disabled={!scopeForProject || doScan.isPending}
          onClick={() => {
            setStage(2);
            doScan.mutate(scopeForProject || defaultScope);
          }}
        >
          <ScanSearch size={14} className="mr-1.5" />
          Escanear
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={!scopeForProject || doPlan.isPending}
          onClick={() => doPlan.mutate()}
        >
          Generar plan
        </Button>
        <Button
          type="button"
          disabled={!plan || plan.session.status === "applied" || doApply.isPending}
          onClick={() => plan && doApply.mutate(plan.session.id)}
        >
          <Play size={14} className="mr-1.5" />
          Aplicar plan
        </Button>
      </div>

      {message && (
        <div className="border-b border-border px-4 py-2 text-[12px] text-muted">
          {message}
        </div>
      )}

      <div className="grid min-h-0 flex-1 grid-cols-2 gap-0">
        <section className="min-h-0 overflow-auto border-r border-border p-4">
          <h2 className="mb-2 text-[12px] font-semibold uppercase text-muted">
            Clasificación
          </h2>
          {(scan ?? []).map((item) => (
            <div key={item.absolute_path} className="mb-3 text-[13px]">
              <div className="font-medium text-text">
                {item.project_name || item.folder_name}
                {item.project_id ? "" : " · no registrado"}
              </div>
              <div className="text-[11px] text-muted">{item.absolute_path}</div>
              <ul className="mt-1 space-y-0.5 text-[12px] text-muted">
                {item.classified
                  .filter((c) => c.matched)
                  .map((c) => (
                    <li key={c.category_key}>
                      {c.category_name}: {c.physical_name} → {c.standard_target}
                    </li>
                  ))}
                {item.unmatched_folders.length > 0 && (
                  <li className="text-[color:var(--color-warn)]">
                    Sin mapear: {item.unmatched_folders.join(", ")}
                  </li>
                )}
              </ul>
            </div>
          ))}
          {!scan && (
            <p className="text-[13px] text-muted">
              Etapa 1–2: elija un proyecto y ejecute escaneo.
            </p>
          )}
        </section>

        <section className="min-h-0 overflow-auto p-4">
          <h2 className="mb-2 text-[12px] font-semibold uppercase text-muted">
            Plan editable {plan ? `(${plan.session.status})` : ""}
          </h2>
          {plan ? (
            <table className="w-full text-left text-[12px]">
              <thead className="text-[11px] uppercase text-muted">
                <tr>
                  <th className="py-1">Proyecto</th>
                  <th className="py-1">Acción</th>
                  <th className="py-1">Origen</th>
                  <th className="py-1">Destino</th>
                  <th className="py-1">Estado</th>
                  <th className="py-1">Editar</th>
                </tr>
              </thead>
              <tbody>
                {plan.items.map((it) => (
                  <tr key={it.id} className="border-t border-border/50">
                    <td className="py-1 text-text">{it.project_folder_name}</td>
                    <td className="py-1 text-muted">{it.action}</td>
                    <td className="py-1 text-muted">
                      {it.source_relative_path || "—"}
                    </td>
                    <td className="py-1 text-muted">
                      {it.target_relative_path || "—"}
                    </td>
                    <td className="py-1 text-muted">{it.status}</td>
                    <td className="py-1">
                      {plan.session.status !== "applied" &&
                        it.status === "pending" && (
                          <button
                            type="button"
                            className="inline-flex items-center gap-1 text-[11px] text-warn hover:underline"
                            onClick={() => void skipItem(it.id)}
                            title="Omitir"
                          >
                            <SkipForward size={12} /> Omitir
                          </button>
                        )}
                      {plan.session.status !== "applied" &&
                        it.status === "skipped" && (
                          <button
                            type="button"
                            className="inline-flex items-center gap-1 text-[11px] text-accent hover:underline"
                            onClick={() =>
                              void restoreItem(
                                it.id,
                                it.action === "skip" ? "map_virtual" : it.action,
                              )
                            }
                          >
                            <Check size={12} /> Restaurar
                          </button>
                        )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <p className="text-[13px] text-muted">
              Etapa 4: genere un plan para revisar y editar (omitir ítems).
            </p>
          )}
        </section>
      </div>
    </div>
  );
}
