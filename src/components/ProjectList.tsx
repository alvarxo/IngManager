import { useEffect, useState, type MouseEvent as ReactMouseEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MoreHorizontal, Plus, Star } from "lucide-react";
import { api } from "@/lib/api";
import type { Project } from "@/lib/types";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";
import { CreateProjectDialog } from "./CreateProjectDialog";
import { RemoveProjectDialog } from "./RemoveProjectDialog";

interface ContextMenuState {
  x: number;
  y: number;
  project: Project;
}

export function ProjectList() {
  const qc = useQueryClient();
  const [createOpen, setCreateOpen] = useState(false);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [removeTarget, setRemoveTarget] = useState<Project | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const view = useUiStore((s) => s.view);
  const setView = useUiStore((s) => s.setView);
  const showArchived = useUiStore((s) => s.showArchived);

  const { data: projects = [], isLoading, error } = useQuery({
    queryKey: ["projects", showArchived],
    queryFn: () => api.listProjects(showArchived),
  });

  const selectedId = view.kind === "project" ? view.projectId : null;

  useEffect(() => {
    function close() {
      setContextMenu(null);
    }
    window.addEventListener("click", close);
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("scroll", close, true);
    };
  }, []);

  async function openProject(project: Project) {
    setActionError(null);
    setView({ kind: "project", projectId: project.id });
    try {
      await api.openProject(project.id);
    } catch (e) {
      setActionError(
        `No se pudo completar la apertura de ${project.code ?? project.name}: ${String(e)}`,
      );
    }
    await qc.invalidateQueries({ queryKey: ["recent-projects"] });
  }

  function openMenuAt(project: Project, x: number, y: number) {
    setContextMenu({ x, y, project });
  }

  function onContextMenu(e: ReactMouseEvent, project: Project) {
    // App menu — never the WebView/browser default.
    e.preventDefault();
    e.stopPropagation();
    openMenuAt(project, e.clientX, e.clientY);
  }

  function onOverflowClick(e: ReactMouseEvent, project: Project) {
    e.preventDefault();
    e.stopPropagation();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenuAt(project, rect.right - 8, rect.bottom + 4);
  }

  const archive = useMutation({
    mutationFn: (id: string) => api.archiveProject(id),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["recent-projects"] });
      setContextMenu(null);
    },
    onError: (e: Error) => {
      setActionError(String(e));
      setContextMenu(null);
    },
  });

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

      {actionError && (
        <div className="border-b border-error/40 bg-error/10 px-3 py-2 text-[11.5px] text-error">
          {actionError}
          <button
            type="button"
            className="ml-2 underline"
            onClick={() => setActionError(null)}
          >
            Cerrar
          </button>
        </div>
      )}

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
                <div
                  className={cn(
                    "flex min-h-[42px] w-full items-center gap-1 border-b border-border/60 px-2 py-1",
                    selectedId === p.id ? "bg-selection" : "hover:bg-elevated",
                  )}
                  onContextMenu={(e) => onContextMenu(e, p)}
                >
                  <button
                    type="button"
                    onClick={() => void openProject(p)}
                    className="flex min-w-0 flex-1 items-center gap-2 px-1 text-left"
                    title={pathHint}
                  >
                    <span className="w-[52px] shrink-0 font-mono text-[12px] font-semibold text-accent">
                      {codeLabel}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[12.5px]">
                        {p.name}
                      </span>
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
                  <button
                    type="button"
                    className="shrink-0 rounded p-1 text-muted hover:bg-panel hover:text-foreground"
                    aria-label={`Más acciones para ${codeLabel}`}
                    title="Más acciones"
                    onClick={(e) => onOverflowClick(e, p)}
                  >
                    <MoreHorizontal size={14} />
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      </div>

      <CreateProjectDialog open={createOpen} onClose={() => setCreateOpen(false)} />

      {removeTarget && (
        <RemoveProjectDialog
          project={removeTarget}
          onClose={() => setRemoveTarget(null)}
        />
      )}

      {contextMenu && (
        <div
          className="fixed z-50 min-w-[220px] border border-border bg-panel py-1 shadow-lg"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
          onContextMenu={(e) => e.preventDefault()}
        >
          <MenuItem
            label="Abrir"
            onClick={() => {
              const p = contextMenu.project;
              setContextMenu(null);
              void openProject(p);
            }}
          />
          <MenuItem
            label="Revelar en explorador"
            onClick={() => {
              const p = contextMenu.project;
              setContextMenu(null);
              void (async () => {
                try {
                  const path = await api.getProjectAbsolutePath(p.id);
                  await api.revealInExplorer(path);
                } catch (e) {
                  setActionError(String(e));
                }
              })();
            }}
          />
          <MenuItem
            label="Archivar"
            disabled={contextMenu.project.status === "archived"}
            onClick={() => archive.mutate(contextMenu.project.id)}
          />
          <div className="my-1 border-t border-border" />
          <MenuItem
            label="Quitar de IngManager…"
            onClick={() => {
              setRemoveTarget(contextMenu.project);
              setContextMenu(null);
            }}
          />
        </div>
      )}
    </aside>
  );
}

function MenuItem({
  label,
  onClick,
  disabled,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      className={cn(
        "block w-full px-3 py-1.5 text-left text-[12.5px]",
        disabled
          ? "cursor-not-allowed text-muted opacity-50"
          : "hover:bg-elevated",
      )}
      onClick={onClick}
    >
      {label}
    </button>
  );
}
