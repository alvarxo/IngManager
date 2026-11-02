import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Plus, Star } from "lucide-react";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";
import { CreateProjectDialog } from "./CreateProjectDialog";

export function ProjectList() {
  const [createOpen, setCreateOpen] = useState(false);
  const view = useUiStore((s) => s.view);
  const setView = useUiStore((s) => s.setView);
  const showArchived = useUiStore((s) => s.showArchived);

  const { data: projects = [], isLoading, error } = useQuery({
    queryKey: ["projects", showArchived],
    queryFn: () => api.listProjects(showArchived),
  });

  const selectedId = view.kind === "project" ? view.projectId : null;

  return (
    <aside className="flex h-full w-full min-w-0 flex-col border-r border-border bg-panel">
      <div className="flex h-[42px] items-center justify-between border-b border-border px-3">
        <span className="text-[12px] font-semibold uppercase tracking-wide text-muted">
          Proyectos
        </span>
        <Button size="sm" onClick={() => setCreateOpen(true)}>
          <Plus size={14} />
          Nuevo
        </Button>
      </div>

      <div className="flex-1 overflow-auto">
        {isLoading && (
          <div className="p-3 text-[12.5px] text-muted">Cargando…</div>
        )}
        {error && (
          <div className="p-3 text-[12.5px] text-error">{String(error)}</div>
        )}
        {!isLoading && projects.length === 0 && (
          <div className="p-4 text-[12.5px] text-muted">
            No hay proyectos. Cree uno nuevo o importe una carpeta existente
            desde Ajustes.
          </div>
        )}
        <ul>
          {projects.map((p) => {
            const codeLabel = p.code?.trim() || "—";
            const pathHint = p.root_relative_path;
            return (
              <li key={p.id}>
                <button
                  type="button"
                  onClick={async () => {
                    await api.openProject(p.id);
                    setView({ kind: "project", projectId: p.id });
                  }}
                  className={cn(
                    "flex min-h-[42px] w-full items-center gap-2 border-b border-border/60 px-3 py-1 text-left",
                    selectedId === p.id
                      ? "bg-selection"
                      : "hover:bg-elevated",
                  )}
                  title={pathHint}
                >
                  <span className="w-[52px] shrink-0 font-mono text-[12px] font-semibold text-accent">
                    {codeLabel}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[12.5px]">{p.name}</span>
                    <span className="block truncate font-mono text-[10.5px] text-muted">
                      {pathHint}
                      {p.import_mode === "linked"
                        ? " · vinculado"
                        : p.import_mode === "copied"
                          ? " · copiado"
                          : ""}
                      {p.layout_mode === "pending_reorganize"
                        ? " · reorg. pendiente"
                        : ""}
                    </span>
                  </span>
                  {p.is_favorite === 1 && (
                    <Star
                      size={12}
                      className="shrink-0 text-warn"
                      fill="currentColor"
                    />
                  )}
                  {p.status !== "active" && (
                    <span className="shrink-0 text-[10px] uppercase text-muted">
                      {p.status}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      </div>

      <CreateProjectDialog open={createOpen} onClose={() => setCreateOpen(false)} />
    </aside>
  );
}
