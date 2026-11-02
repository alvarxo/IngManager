import { useQuery } from "@tanstack/react-query";
import { FolderOpen } from "lucide-react";
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

  const audit = useQuery({
    queryKey: ["project-audit", projectId],
    queryFn: () => api.listProjectAudit(projectId, 200),
    enabled: projectMainTab === "log",
  });

  const favDocs = useQuery({
    queryKey: ["favorite-documents", projectId],
    queryFn: () => api.listFavoriteDocuments(projectId),
    enabled: projectMainTab === "info",
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
        </div>
      </div>

      <div className="flex border-b border-border px-2">
        {(
          [
            ["documents", "Documentos"],
            ["transmissions", "Transmisiones"],
            ["info", "Info"],
            ["log", "Bitácora"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => setProjectMainTab(id)}
            className={cn(
              "h-[36px] px-3 text-[12.5px]",
              projectMainTab === id
                ? "border-b-2 border-accent text-text"
                : "text-muted hover:text-text",
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
      ) : projectMainTab === "info" ? (
        <div className="min-h-0 flex-1 overflow-auto p-6">
          <h2 className="mb-3 text-[15px] font-semibold">Información del proyecto</h2>
          <dl className="mb-6 grid max-w-xl gap-3 text-[13px]">
            <InfoRow label="Código" value={p.code} mono />
            <InfoRow label="Nombre" value={p.name} />
            <InfoRow label="Cliente" value={p.client_name ?? "—"} />
            <InfoRow label="Estado" value={p.status} />
            <InfoRow label="Descripción" value={p.description ?? "—"} />
            <InfoRow label="Inicio" value={p.start_date ?? "—"} />
            <InfoRow label="Fin" value={p.end_date ?? "—"} />
            <InfoRow label="Ruta relativa" value={p.root_relative_path} mono />
            <InfoRow
              label="Última apertura"
              value={
                p.last_opened_at
                  ? new Date(p.last_opened_at).toLocaleString()
                  : "—"
              }
            />
            <InfoRow
              label="Creado"
              value={new Date(p.created_at).toLocaleString()}
            />
          </dl>
          <h3 className="mb-2 text-[12px] font-semibold uppercase text-muted">
            Documentos favoritos
          </h3>
          {(favDocs.data?.length ?? 0) === 0 ? (
            <p className="text-[12.5px] text-muted">
              Sin documentos favoritos en este proyecto.
            </p>
          ) : (
            <ul className="max-w-xl space-y-1">
              {favDocs.data?.map((d) => (
                <li key={d.id}>
                  <button
                    type="button"
                    className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12.5px] hover:bg-elevated"
                    onClick={() => {
                      setProjectMainTab("documents");
                      setSelectedDocumentId(d.id);
                      if (d.category_key) {
                        setView({
                          kind: "project",
                          projectId,
                          categoryKey: d.category_key,
                        });
                      }
                    }}
                  >
                    <span className="font-mono text-accent">
                      {d.business_code ?? "—"}
                    </span>
                    <span className="truncate">{d.title}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : projectMainTab === "log" ? (
        <div className="min-h-0 flex-1 overflow-auto p-4">
          <h2 className="mb-3 text-[15px] font-semibold">Bitácora</h2>
          {audit.isLoading && (
            <p className="text-[12.5px] text-muted">Cargando eventos…</p>
          )}
          {audit.error && (
            <p className="text-[12.5px] text-error">{String(audit.error)}</p>
          )}
          {(audit.data?.length ?? 0) === 0 && !audit.isLoading ? (
            <p className="text-[12.5px] text-muted">
              Sin eventos registrados para este proyecto.
            </p>
          ) : (
            <ul className="space-y-1">
              {audit.data?.map((e) => (
                <li
                  key={e.id}
                  className="border-b border-border/50 px-1 py-2 text-[12.5px]"
                >
                  <div className="flex items-baseline justify-between gap-3">
                    <span className="font-mono text-accent">{e.action}</span>
                    <span className="shrink-0 text-[11px] text-muted">
                      {new Date(e.created_at).toLocaleString()}
                    </span>
                  </div>
                  <div className="text-[11.5px] text-muted">
                    {e.entity_type ?? "—"}
                    {e.entity_id ? ` · ${e.entity_id.slice(0, 8)}…` : ""}
                  </div>
                  {e.detail && (
                    <div className="mt-0.5 text-[12px] text-muted">{e.detail}</div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
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

function InfoRow({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div>
      <dt className="text-[10.5px] font-medium uppercase tracking-wide text-muted">
        {label}
      </dt>
      <dd className={cn("text-[13px]", mono && "font-mono text-[12.5px]")}>
        {value}
      </dd>
    </div>
  );
}
