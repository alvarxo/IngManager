import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Star, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { formatBytes } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";
import { cn } from "@/lib/utils";
import { DocumentInspector } from "./DocumentInspector";

export function Inspector() {
  const view = useUiStore((s) => s.view);
  const selectedPaths = useUiStore((s) => s.selectedPaths);
  const selectedDocumentId = useUiStore((s) => s.selectedDocumentId);
  const inspectorTab = useUiStore((s) => s.inspectorTab);
  const setInspectorTab = useUiStore((s) => s.setInspectorTab);
  const qc = useQueryClient();

  const projectId = view.kind === "project" ? view.projectId : null;
  const project = useQuery({
    queryKey: ["project", projectId],
    queryFn: () => api.getProject(projectId!),
    enabled: !!projectId,
  });

  const selectedPath = selectedPaths[0] ?? null;
  const props = useQuery({
    queryKey: ["entry-props", selectedPath],
    queryFn: () => api.getEntryProperties(selectedPath!),
    enabled: !!selectedPath && inspectorTab === "file",
  });

  const trash = useQuery({
    queryKey: ["trash"],
    queryFn: () => api.listTrash(),
    enabled: inspectorTab === "trash",
  });

  const favMutation = useMutation({
    mutationFn: (favorite: boolean) =>
      api.updateProject({ id: projectId!, is_favorite: favorite }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["favorite-projects"] });
    },
  });

  const archiveMutation = useMutation({
    mutationFn: () => api.archiveProject(projectId!),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
    },
  });

  const unarchiveMutation = useMutation({
    mutationFn: () =>
      api.updateProject({ id: projectId!, status: "active" }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
    },
  });

  const foldersFlagMutation = useMutation({
    mutationFn: (enabled: boolean) =>
      api.updateProject({
        id: projectId!,
        physical_revision_folders: enabled,
      }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
      await qc.invalidateQueries({ queryKey: ["projects"] });
    },
  });

  const reorgPreviewMutation = useMutation({
    mutationFn: () => api.previewRevisionFolderReorg(projectId!),
    onSuccess: (data) => {
      setReorgError(null);
      setReorgResult(null);
      setReorgPreview(data);
    },
    onError: (e: Error) => {
      setReorgPreview(null);
      setReorgError(String(e));
    },
  });

  const reorgApplyMutation = useMutation({
    mutationFn: () =>
      api.applyRevisionFolderReorg(projectId!, reorgPreview!.session_id),
    onSuccess: async (data) => {
      setReorgResult(
        `Movidos: ${data.moved} · Omitidos: ${data.skipped} · Fallidos: ${data.failed}`,
      );
      setReorgPreview(null);
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
      await qc.invalidateQueries({ queryKey: ["documents"] });
      await qc.invalidateQueries({ queryKey: ["category-files"] });
    },
    onError: (e: Error) => setReorgError(String(e)),
  });

  const [editingProject, setEditingProject] = useState(false);
  const [editName, setEditName] = useState("");
  const [editClient, setEditClient] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [projectMetaError, setProjectMetaError] = useState<string | null>(null);
  const [reorgPreview, setReorgPreview] = useState<
    Awaited<ReturnType<typeof api.previewRevisionFolderReorg>> | null
  >(null);
  const [reorgError, setReorgError] = useState<string | null>(null);
  const [reorgResult, setReorgResult] = useState<string | null>(null);

  const projectMetaMutation = useMutation({
    mutationFn: () =>
      api.updateProject({
        id: projectId!,
        name: editName.trim() || undefined,
        client_name: editClient.trim() || null,
        description: editDescription.trim() || null,
      }),
    onSuccess: async () => {
      setEditingProject(false);
      setProjectMetaError(null);
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
      await qc.invalidateQueries({ queryKey: ["projects"] });
    },
    onError: (e: Error) => setProjectMetaError(String(e)),
  });

  const isDocTab = inspectorTab.startsWith("doc-");

  return (
    <aside className="flex h-full w-full min-w-0 flex-col border-l border-border bg-panel">
      <div className="flex border-b border-border">
        {(
          [
            ["project", "Proyecto"],
            ["doc-detail", "Documento"],
            ["file", "Archivo"],
            ["trash", "Papelera"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => setInspectorTab(id)}
            className={cn(
              "h-[36px] flex-1 text-[11.5px]",
              (id === "doc-detail" ? isDocTab : inspectorTab === id)
                ? "border-b-2 border-accent text-text"
                : "text-muted hover:text-text",
            )}
          >
            {label}
            {id === "doc-detail" && selectedDocumentId ? " ·" : ""}
          </button>
        ))}
      </div>

      {isDocTab ? (
        <DocumentInspector />
      ) : (
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {inspectorTab === "project" && (
            <>
              {!projectId && (
                <p className="text-[12.5px] text-muted">
                  Seleccione un proyecto para ver detalles.
                </p>
              )}
              {projectId && project.isLoading && (
                <p className="text-muted">Cargando…</p>
              )}
              {project.data && (
                <div className="space-y-4">
                  <div>
                    <div className="font-mono text-[15px] font-semibold text-accent">
                      {project.data.code ?? "—"}
                    </div>
                    {!editingProject ? (
                      <div className="text-[15px] font-medium">
                        {project.data.name}
                      </div>
                    ) : null}
                  </div>
                  {!editingProject ? (
                    <>
                      <Meta label="Cliente" value={project.data.client_name ?? "—"} />
                      <Meta label="Estado" value={project.data.status} />
                      <Meta
                        label="Ruta"
                        value={project.data.root_relative_path}
                        mono
                      />
                      <Meta
                        label="Descripción"
                        value={project.data.description ?? "—"}
                      />
                      <p className="text-[11px] text-muted">
                        La ruta física no cambia al editar metadatos.
                      </p>
                    </>
                  ) : (
                    <div className="space-y-2">
                      <label className="block">
                        <div className="mb-1 text-[11px] uppercase text-muted">Nombre</div>
                        <input
                          className="h-[32px] w-full border border-border bg-elevated px-2 text-[13px]"
                          value={editName}
                          onChange={(e) => setEditName(e.target.value)}
                        />
                      </label>
                      <label className="block">
                        <div className="mb-1 text-[11px] uppercase text-muted">Cliente</div>
                        <input
                          className="h-[32px] w-full border border-border bg-elevated px-2 text-[13px]"
                          value={editClient}
                          onChange={(e) => setEditClient(e.target.value)}
                        />
                      </label>
                      <label className="block">
                        <div className="mb-1 text-[11px] uppercase text-muted">Descripción</div>
                        <textarea
                          className="min-h-[64px] w-full border border-border bg-elevated px-2 py-1 text-[13px]"
                          value={editDescription}
                          onChange={(e) => setEditDescription(e.target.value)}
                        />
                      </label>
                      <Meta
                        label="Ruta (solo lectura)"
                        value={project.data.root_relative_path}
                        mono
                      />
                      {projectMetaError && (
                        <div className="border border-error/40 bg-error/10 px-2 py-1.5 text-[12px] text-error">
                          {projectMetaError}
                        </div>
                      )}
                      <div className="flex flex-wrap gap-1">
                        <Button
                          size="sm"
                          disabled={projectMetaMutation.isPending}
                          onClick={() => projectMetaMutation.mutate()}
                        >
                          Guardar
                        </Button>
                        <Button
                          size="sm"
                          variant="secondary"
                          onClick={() => setEditingProject(false)}
                        >
                          Cancelar
                        </Button>
                      </div>
                    </div>
                  )}
                  <div className="flex flex-wrap gap-2 pt-2">
                    {!editingProject && (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => {
                          setEditName(project.data!.name);
                          setEditClient(project.data!.client_name ?? "");
                          setEditDescription(project.data!.description ?? "");
                          setProjectMetaError(null);
                          setEditingProject(true);
                        }}
                      >
                        Editar metadatos…
                      </Button>
                    )}
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() =>
                        favMutation.mutate(project.data!.is_favorite !== 1)
                      }
                    >
                      <Star
                        size={14}
                        className={
                          project.data.is_favorite === 1
                            ? "text-warn"
                            : undefined
                        }
                        fill={
                          project.data.is_favorite === 1
                            ? "currentColor"
                            : "none"
                        }
                      />
                      Favorito
                    </Button>
                    {project.data.status === "active" && (
                      <Button
                        variant="danger"
                        size="sm"
                        onClick={() => archiveMutation.mutate()}
                      >
                        Archivar
                      </Button>
                    )}
                    {project.data.status === "archived" && (
                      <Button
                        size="sm"
                        disabled={unarchiveMutation.isPending}
                        onClick={() => unarchiveMutation.mutate()}
                      >
                        Desarchivar
                      </Button>
                    )}
                  </div>

                  <div className="space-y-2 border-t border-border pt-3">
                    <div className="text-[11px] uppercase text-muted">
                      Carpetas por revisión
                    </div>
                    <label className="flex items-start gap-2 text-[12.5px]">
                      <input
                        type="checkbox"
                        className="mt-0.5"
                        checked={project.data.physical_revision_folders === 1}
                        disabled={foldersFlagMutation.isPending}
                        onChange={(e) =>
                          foldersFlagMutation.mutate(e.target.checked)
                        }
                      />
                      <span>
                        Reorganizar por carpetas de revisión
                        <span className="mt-0.5 block text-[11px] text-muted">
                          Nuevas emisiones →{" "}
                          <span className="font-mono">CATEGORIA/RA/</span> (modelos:{" "}
                          <span className="font-mono">v01/</span>). No mueve
                          archivos existentes.
                        </span>
                      </span>
                    </label>
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={reorgPreviewMutation.isPending}
                      onClick={() => reorgPreviewMutation.mutate()}
                    >
                      Reorganizar archivos…
                    </Button>
                    {reorgError && (
                      <div className="border border-error/40 bg-error/10 px-2 py-1.5 text-[12px] text-error">
                        {reorgError}
                      </div>
                    )}
                    {reorgResult && (
                      <p className="text-[12px] text-muted">{reorgResult}</p>
                    )}
                    {reorgPreview && (
                      <div className="space-y-2 border border-border bg-elevated p-2 text-[12px]">
                        <p>
                          Vista previa:{" "}
                          <strong>{reorgPreview.to_move}</strong> a mover,{" "}
                          {reorgPreview.skipped} omitidos.
                        </p>
                        <ul className="max-h-[160px] space-y-1 overflow-auto font-mono text-[11px]">
                          {reorgPreview.items
                            .filter((i) => i.action !== "skip")
                            .slice(0, 40)
                            .map((i) => (
                              <li key={i.document_file_id}>
                                {i.source_relative_path} →{" "}
                                {i.dest_relative_path}
                              </li>
                            ))}
                        </ul>
                        <div className="flex flex-wrap gap-1">
                          <Button
                            size="sm"
                            disabled={
                              reorgApplyMutation.isPending ||
                              reorgPreview.to_move === 0
                            }
                            onClick={() => reorgApplyMutation.mutate()}
                          >
                            Aplicar movimientos
                          </Button>
                          <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => setReorgPreview(null)}
                          >
                            Cancelar
                          </Button>
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              )}
            </>
          )}

          {inspectorTab === "file" && (
            <>
              {!selectedPath && (
                <p className="text-[12.5px] text-muted">
                  Seleccione un archivo o carpeta en el administrador.
                </p>
              )}
              {selectedPath && props.isLoading && (
                <p className="text-muted">Cargando propiedades…</p>
              )}
              {props.error && (
                <p className="text-error">{String(props.error)}</p>
              )}
              {props.data && (
                <div className="space-y-3">
                  <div className="text-[14px] font-medium">{props.data.name}</div>
                  <Meta
                    label="Tipo"
                    value={props.data.is_directory ? "Carpeta" : "Archivo"}
                  />
                  <Meta label="Extensión" value={props.data.extension ?? "—"} />
                  <Meta
                    label="Tamaño"
                    value={formatBytes(props.data.size_bytes)}
                  />
                  <Meta
                    label="Ruta relativa"
                    value={props.data.relative_path}
                    mono
                  />
                </div>
              )}
            </>
          )}

          {inspectorTab === "trash" && (
            <div className="space-y-3">
              <p className="text-[12px] text-muted">
                Papelera propia de IngManager (bajo .ingmanager/trash).
              </p>
              {trash.isLoading && <p className="text-muted">Cargando…</p>}
              {(trash.data?.length ?? 0) === 0 && !trash.isLoading && (
                <p className="text-[12.5px] text-muted">Papelera vacía.</p>
              )}
              <ul className="space-y-2">
                {trash.data?.map((item) => (
                  <li
                    key={item.id}
                    className="border border-border bg-elevated p-2 text-[12px]"
                  >
                    <div className="font-medium">
                      {item.name ?? item.original_path}
                    </div>
                    <div className="truncate text-muted">{item.original_path}</div>
                    <div className="mt-2 flex gap-1">
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={async () => {
                          await api.restoreTrashItem(item.id);
                          await qc.invalidateQueries({ queryKey: ["trash"] });
                          await qc.invalidateQueries({
                            queryKey: ["fm-entries"],
                          });
                        }}
                      >
                        Restaurar
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        onClick={async () => {
                          if (!window.confirm("¿Eliminar permanentemente?"))
                            return;
                          await api.purgeTrashItem(item.id);
                          await qc.invalidateQueries({ queryKey: ["trash"] });
                        }}
                      >
                        <Trash2 size={12} /> Purgar
                      </Button>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </aside>
  );
}

function Meta({
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
      <div className="text-[11px] font-medium uppercase tracking-wide text-muted">
        {label}
      </div>
      <div className={mono ? "break-all font-mono text-[12px]" : "text-[13px]"}>
        {value}
      </div>
    </div>
  );
}
