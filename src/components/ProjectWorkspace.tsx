import { useQuery } from "@tanstack/react-query";
import { FolderOpen, MoreHorizontal } from "lucide-react";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";
import { FileManager } from "./FileManager";
import { DocumentBrowser } from "./DocumentBrowser";
import { TransmissionsPanel } from "./TransmissionsPanel";

interface Props {
  projectId: string;
  categoryKey?: string;
}

export function ProjectWorkspace({ projectId, categoryKey }: Props) {
  const setView = useUiStore((s) => s.setView);
  const projectContentMode = useUiStore((s) => s.projectContentMode);
  const setProjectContentMode = useUiStore((s) => s.setProjectContentMode);
  const projectMainTab = useUiStore((s) => s.projectMainTab);
  const setProjectMainTab = useUiStore((s) => s.setProjectMainTab);
  const setSelectedDocumentId = useUiStore((s) => s.setSelectedDocumentId);
  const setSelectedTransmissionId = useUiStore(
    (s) => s.setSelectedTransmissionId,
  );

  const project = useQuery({
    queryKey: ["project", projectId],
    queryFn: () => api.getProject(projectId),
  });

  const categories = useQuery({
    queryKey: ["categories", projectId],
    queryFn: () => api.listProjectCategories(projectId),
  });

  const activeKey = categoryKey ?? categories.data?.[0]?.key;

  if (project.isLoading) {
    return <div className="p-4 text-muted">Cargando proyecto…</div>;
  }
  if (project.error || !project.data) {
    return (
      <div className="p-4 text-error">
        {String(project.error ?? "Proyecto no encontrado")}
      </div>
    );
  }

  const p = project.data;

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col bg-bg">
      <div className="flex h-[42px] items-center gap-3 border-b border-border px-4">
        <span className="font-mono text-[13px] font-semibold text-accent">
          {p.code}
        </span>
        <span className="truncate text-[14px] font-medium">{p.name}</span>
        <span className="rounded-[4px] border border-border px-1.5 py-0.5 text-[10px] uppercase text-muted">
          {p.status}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => api.openProjectFolder(p.id)}
          >
            <FolderOpen size={14} />
            Abrir carpeta
          </Button>
          <Button
            variant="ghost"
            size="icon"
            disabled
            title="Más acciones (próximas fases)"
          >
            <MoreHorizontal size={16} />
          </Button>
        </div>
      </div>

      <div className="flex border-b border-border px-2">
        {(
          [
            ["documents", "Documentos", true],
            ["transmissions", "Transmisiones", true],
            ["info", "Info", false],
            ["log", "Bitácora", false],
          ] as const
        ).map(([id, label, enabled]) => (
          <button
            key={id}
            type="button"
            disabled={!enabled}
            title={!enabled ? "Disponible en fases posteriores" : undefined}
            onClick={() => {
              if (!enabled) return;
              if (id === "documents" || id === "transmissions") {
                setProjectMainTab(id);
              }
            }}
            className={cn(
              "h-[36px] px-3 text-[12.5px]",
              enabled &&
                ((id === "documents" && projectMainTab === "documents") ||
                  (id === "transmissions" && projectMainTab === "transmissions"))
                ? "border-b-2 border-accent text-text"
                : enabled
                  ? "text-muted hover:text-text"
                  : "cursor-not-allowed text-muted opacity-50",
            )}
          >
            {label}
          </button>
        ))}
        {projectMainTab === "documents" && (
          <div className="ml-auto flex items-center gap-1 px-2">
            <Button
              size="sm"
              variant={projectContentMode === "documents" ? "default" : "ghost"}
              onClick={() => setProjectContentMode("documents")}
            >
              Lógicos
            </Button>
            <Button
              size="sm"
              variant={projectContentMode === "files" ? "default" : "ghost"}
              onClick={() => {
                setProjectContentMode("files");
                setSelectedDocumentId(null);
              }}
            >
              Archivos
            </Button>
          </div>
        )}
      </div>

      {projectMainTab === "transmissions" ? (
        <TransmissionsPanel projectId={projectId} />
      ) : (
        <div className="flex min-h-0 flex-1">
          <div className="w-[200px] shrink-0 overflow-auto border-r border-border bg-panel">
            <div className="px-3 py-2 text-[11px] font-semibold uppercase text-muted">
              Categorías
            </div>
            <ul>
              {categories.data?.map((c) => (
                <li key={c.key}>
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedDocumentId(null);
                      setSelectedTransmissionId(null);
                      setView({
                        kind: "project",
                        projectId,
                        categoryKey: c.key,
                      });
                    }}
                    className={cn(
                      "flex h-[38px] w-full items-center justify-between px-3 text-left text-[12.5px]",
                      activeKey === c.key
                        ? "bg-selection text-text"
                        : "text-muted hover:bg-elevated hover:text-text",
                    )}
                  >
                    <span>{c.name}</span>
                    <span className="text-[11px] tabular-nums text-muted">
                      {c.file_count}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </div>

          {activeKey ? (
            projectContentMode === "documents" ? (
              <DocumentBrowser projectId={projectId} categoryKey={activeKey} />
            ) : (
              <FileManager projectId={projectId} categoryKey={activeKey} />
            )
          ) : (
            <div className="p-4 text-muted">Sin categorías</div>
          )}
        </div>
      )}
    </div>
  );
}
