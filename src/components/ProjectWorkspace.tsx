import { useQuery } from "@tanstack/react-query";
import {
  FileText,
  FolderOpen,
  ExternalLink,
  MoreHorizontal,
} from "lucide-react";
import { api } from "@/lib/api";
import { cn, formatBytes } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";

interface Props {
  projectId: string;
  categoryKey?: string;
}

export function ProjectWorkspace({ projectId, categoryKey }: Props) {
  const setView = useUiStore((s) => s.setView);

  const project = useQuery({
    queryKey: ["project", projectId],
    queryFn: () => api.getProject(projectId),
  });

  const categories = useQuery({
    queryKey: ["categories", projectId],
    queryFn: () => api.listProjectCategories(projectId),
  });

  const activeKey = categoryKey ?? categories.data?.[0]?.key;
  const files = useQuery({
    queryKey: ["files", projectId, activeKey],
    queryFn: () => api.listCategoryFiles(projectId, activeKey!),
    enabled: !!activeKey,
  });

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
          <Button variant="ghost" size="icon" disabled title="Más acciones (próximas fases)">
            <MoreHorizontal size={16} />
          </Button>
        </div>
      </div>

      <div className="flex border-b border-border px-2">
        {["Documentos", "Transmisiones", "Info", "Bitácora"].map((tab, i) => (
          <button
            key={tab}
            type="button"
            disabled={i > 0}
            title={i > 0 ? "Disponible en fases posteriores" : undefined}
            className={cn(
              "h-[36px] px-3 text-[12.5px]",
              i === 0
                ? "border-b-2 border-accent text-text"
                : "cursor-not-allowed text-muted opacity-50",
            )}
          >
            {tab}
          </button>
        ))}
      </div>

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
                  onClick={() =>
                    setView({
                      kind: "project",
                      projectId,
                      categoryKey: c.key,
                    })
                  }
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

        <div className="min-w-0 flex-1 overflow-auto">
          <table className="w-full border-collapse text-left">
            <thead className="sticky top-0 bg-panel">
              <tr className="border-b border-border text-[11px] uppercase text-muted">
                <th className="h-[36px] px-3 font-medium">Nombre</th>
                <th className="w-[80px] px-3 font-medium">Ext</th>
                <th className="w-[90px] px-3 font-medium">Tamaño</th>
                <th className="w-[160px] px-3 font-medium">Modificado</th>
                <th className="w-[100px] px-3 font-medium">Acciones</th>
              </tr>
            </thead>
            <tbody>
              {files.isLoading && (
                <tr>
                  <td colSpan={5} className="px-3 py-4 text-muted">
                    Cargando archivos…
                  </td>
                </tr>
              )}
              {!files.isLoading && (files.data?.length ?? 0) === 0 && (
                <tr>
                  <td colSpan={5} className="px-3 py-8 text-center text-muted">
                    <FileText className="mx-auto mb-2 opacity-40" size={28} />
                    Sin archivos indexados en esta categoría.
                    <div className="mt-1 text-[11.5px]">
                      Coloque archivos en la carpeta física y use Reindexar en
                      Ajustes.
                    </div>
                  </td>
                </tr>
              )}
              {files.data?.map((f) => (
                <tr
                  key={f.id}
                  className="h-[40px] border-b border-border/50 hover:bg-elevated"
                  onDoubleClick={() => api.openFile(f.absolute_path)}
                >
                  <td className="truncate px-3">{f.file_name}</td>
                  <td className="px-3 text-muted">{f.extension ?? "—"}</td>
                  <td className="px-3 tabular-nums text-muted">
                    {formatBytes(f.size_bytes)}
                  </td>
                  <td className="px-3 text-[12px] text-muted">
                    {f.modified_at
                      ? new Date(f.modified_at).toLocaleString()
                      : "—"}
                  </td>
                  <td className="px-3">
                    <div className="flex gap-1">
                      <Button
                        variant="ghost"
                        size="icon"
                        title="Abrir"
                        onClick={() => api.openFile(f.absolute_path)}
                      >
                        <ExternalLink size={14} />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        title="Revelar en explorador"
                        onClick={() => api.revealInExplorer(f.absolute_path)}
                      >
                        <FolderOpen size={14} />
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
