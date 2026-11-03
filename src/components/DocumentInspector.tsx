import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Star } from "lucide-react";
import { api } from "@/lib/api";
import { formatBytes } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";
import { cn } from "@/lib/utils";

const RELATION_TYPES = [
  "relacionado",
  "deriva_de",
  "reemplaza",
  "referencia",
  "anexo",
];

export function DocumentInspector() {
  const documentId = useUiStore((s) => s.selectedDocumentId);
  const inspectorTab = useUiStore((s) => s.inspectorTab);
  const setInspectorTab = useUiStore((s) => s.setInspectorTab);
  const setSelectedDocumentId = useUiStore((s) => s.setSelectedDocumentId);
  const showCurrentVersionOnly = useUiStore((s) => s.showCurrentVersionOnly);
  const qc = useQueryClient();

  const doc = useQuery({
    queryKey: ["document", documentId],
    queryFn: () => api.getDocument(documentId!),
    enabled: !!documentId,
  });

  const favorite = useQuery({
    queryKey: ["doc-favorite", documentId],
    queryFn: () => api.isDocumentFavorite(documentId!),
    enabled: !!documentId,
  });

  const versions = useQuery({
    queryKey: ["doc-versions", documentId],
    queryFn: () => api.listDocumentVersions(documentId!),
    enabled: !!documentId && inspectorTab === "doc-versions",
  });

  const revisions = useQuery({
    queryKey: ["doc-revisions", documentId],
    queryFn: () => api.listRevisions(documentId!),
    enabled: !!documentId && inspectorTab === "doc-versions",
  });

  const related = useQuery({
    queryKey: ["doc-related", documentId],
    queryFn: () => api.listDocumentRelationships(documentId!),
    enabled: !!documentId && inspectorTab === "doc-related",
  });

  const projectDocs = useQuery({
    queryKey: ["documents-all", doc.data?.project_id],
    queryFn: () => api.listDocuments(doc.data!.project_id, null, true),
    enabled: !!doc.data?.project_id && inspectorTab === "doc-related",
  });

  const [selectedVersionId, setSelectedVersionId] = useState<string | null>(null);
  const [relTarget, setRelTarget] = useState("");
  const [relType, setRelType] = useState("relacionado");
  const [relNotes, setRelNotes] = useState("");
  const [relError, setRelError] = useState<string | null>(null);
  const [versionError, setVersionError] = useState<string | null>(null);
  const [versionBusy, setVersionBusy] = useState(false);

  const activeVersionId =
    selectedVersionId ??
    versions.data?.find((v) => v.is_current === 1)?.id ??
    versions.data?.[0]?.id ??
    null;

  const files = useQuery({
    queryKey: ["doc-version-files", activeVersionId],
    queryFn: () => api.listVersionFiles(activeVersionId!),
    enabled: !!activeVersionId && inspectorTab === "doc-versions",
  });

  const updateMut = useMutation({
    mutationFn: (status: string) =>
      api.updateDocument({ id: documentId!, status }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["document", documentId] });
      await qc.invalidateQueries({ queryKey: ["documents"] });
    },
  });

  const favMut = useMutation({
    mutationFn: (value: boolean) =>
      api.setDocumentFavorite(documentId!, value),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["doc-favorite", documentId] });
      await qc.invalidateQueries({ queryKey: ["favorite-documents"] });
    },
  });

  const createRelMut = useMutation({
    mutationFn: () =>
      api.createDocumentRelationship({
        source_document_id: documentId!,
        target_document_id: relTarget,
        relation_type: relType,
        notes: relNotes || null,
      }),
    onSuccess: async () => {
      setRelTarget("");
      setRelNotes("");
      setRelError(null);
      await qc.invalidateQueries({ queryKey: ["doc-related", documentId] });
    },
    onError: (e: Error) => setRelError(String(e)),
  });

  if (!documentId) {
    return (
      <p className="text-[12.5px] text-muted">
        Seleccione un documento lógico para ver detalle, versiones y revisiones.
      </p>
    );
  }

  if (doc.isLoading) return <p className="text-muted">Cargando documento…</p>;
  if (doc.error || !doc.data)
    return <p className="text-error">{String(doc.error ?? "No encontrado")}</p>;

  const d = doc.data;
  const isPlanosOrMemorias =
    d.category_key === "planos" || d.category_key === "memorias";

  return (
    <div className="flex h-full flex-col">
      <div className="flex border-b border-border">
        {(
          [
            ["doc-detail", "Detalle"],
            ["doc-versions", isPlanosOrMemorias ? "Versiones / Rev." : "Versiones"],
            ["doc-related", "Relacionados"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => setInspectorTab(id)}
            className={cn(
              "h-[34px] flex-1 px-1 text-[11.5px]",
              inspectorTab === id
                ? "border-b-2 border-accent text-text"
                : "text-muted",
            )}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3">
        {inspectorTab === "doc-detail" && (
          <>
            <div className="flex items-start justify-between gap-2">
              <div className="font-mono text-[13px] text-accent">
                {d.business_code ?? "(sin código)"}
              </div>
              <Button
                size="sm"
                variant="ghost"
                title={
                  favorite.data ? "Quitar de favoritos" : "Marcar favorito"
                }
                onClick={() => favMut.mutate(!favorite.data)}
              >
                <Star
                  size={14}
                  className={
                    favorite.data ? "fill-accent text-accent" : "text-muted"
                  }
                />
              </Button>
            </div>
            <div className="text-[14px] font-medium">{d.title}</div>
            {d.has_unregistered_change === 1 && (
              <div className="border border-warn/40 bg-warn/10 px-2 py-1.5 text-[12px] text-warn">
                Cambio detectado en archivos físicos vs versión registrada. Registre
                una nueva versión si corresponde.
              </div>
            )}
            <Meta label="Tipo" value={d.doc_type ?? "—"} />
            <Meta label="Estado" value={d.status} />
            <Meta label="Versión actual" value={d.current_version_label ?? "—"} />
            <Meta label="Subcategoría" value={d.subcategory ?? "—"} />
            <Meta label="Origen" value={d.origin ?? "—"} />
            <Meta label="Recepción" value={d.received_at ?? "—"} />
            <Meta label="Notas" value={d.notes ?? "—"} />
            {d.category_key === "memorias" && (
              <label className="block">
                <div className="mb-1 text-[11px] uppercase text-muted">Cambiar estado</div>
                <select
                  className="h-[32px] w-full border border-border bg-elevated px-2"
                  value={d.status}
                  onChange={(e) => updateMut.mutate(e.target.value)}
                >
                  {["desarrollo", "revision", "aprobada", "obsoleta"].map((s) => (
                    <option key={s} value={s}>
                      {s}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <div className="flex flex-wrap gap-1 pt-1">
              <Button
                size="sm"
                variant="secondary"
                onClick={async () => {
                  try {
                    setVersionError(null);
                    const picked = await open({
                      multiple: false,
                      title: "Adjuntar archivo a versión actual",
                    });
                    if (typeof picked !== "string" || !d.current_version_id) return;
                    await api.attachFileToVersion({
                      document_version_id: d.current_version_id,
                      absolute_path: picked,
                      as_snapshot: false,
                    });
                    await qc.invalidateQueries({ queryKey: ["document", documentId] });
                    await qc.invalidateQueries({ queryKey: ["documents"] });
                    await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
                  } catch (e) {
                    setVersionError(String(e));
                  }
                }}
              >
                Adjuntar archivo
              </Button>
              <Button
                size="sm"
                disabled={versionBusy}
                onClick={async () => {
                  try {
                    setVersionBusy(true);
                    setVersionError(null);
                    const picked = await open({
                      multiple: false,
                      title:
                        "Vincular archivo existente como nueva versión (sin copiar). Cancelar = clonar versión actual.",
                    });
                    if (typeof picked === "string") {
                      await api.createDocumentVersion({
                        document_id: d.id,
                        source_absolute_path: picked,
                        from_current: false,
                        notes: "Nueva versión vinculada a archivo existente",
                      });
                    } else {
                      await api.createDocumentVersion({
                        document_id: d.id,
                        from_current: true,
                        notes: "Nueva versión registrada",
                      });
                    }
                    await qc.invalidateQueries({ queryKey: ["document", documentId] });
                    await qc.invalidateQueries({ queryKey: ["documents"] });
                    await qc.invalidateQueries({ queryKey: ["doc-versions", documentId] });
                    await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
                    setInspectorTab("doc-versions");
                  } catch (e) {
                    setVersionError(String(e));
                  } finally {
                    setVersionBusy(false);
                  }
                }}
              >
                Nueva versión
              </Button>
            </div>
            {versionError && (
              <div className="border border-error/40 bg-error/10 px-2 py-1.5 text-[12px] text-error">
                {versionError}
              </div>
            )}
          </>
        )}

        {inspectorTab === "doc-versions" && (
          <>
            <p className="text-[11px] text-muted">
              Versión = iteración de trabajo (archivos vinculados). Revisión
              emitida = entrega con código (formatos en snapshot).
            </p>
            <div className="text-[11px] font-semibold uppercase text-muted">
              Versiones internas
            </div>
            <ul className="space-y-1">
              {(showCurrentVersionOnly
                ? versions.data?.filter((v) => v.is_current === 1)
                : versions.data
              )?.map((v) => (
                <li key={v.id}>
                  <button
                    type="button"
                    className={cn(
                      "flex w-full items-center justify-between rounded-[4px] px-2 py-1.5 text-left text-[12.5px]",
                      activeVersionId === v.id ? "bg-selection" : "hover:bg-elevated",
                    )}
                    onClick={() => setSelectedVersionId(v.id)}
                  >
                    <span className="font-mono">
                      {v.label ?? `v${v.version_number}`}
                      {v.is_current === 1 ? " · actual" : ""}
                    </span>
                    <span className="text-[11px] text-muted">
                      {new Date(v.created_at).toLocaleString()}
                    </span>
                  </button>
                </li>
              ))}
            </ul>

            <div className="text-[11px] font-semibold uppercase text-muted">
              Archivos de la versión
            </div>
            {(files.data?.length ?? 0) === 0 && (
              <p className="text-[12px] text-muted">Sin archivos en esta versión.</p>
            )}
            <ul className="space-y-1">
              {files.data?.map((f) => (
                <li
                  key={f.id}
                  className="flex items-center justify-between gap-2 border border-border/60 px-2 py-1 text-[12px]"
                >
                  <div className="min-w-0 truncate">
                    {f.file_name}
                    {f.format_label && (
                      <span className="ml-1 text-muted">[{f.format_label}]</span>
                    )}
                    {f.is_available !== 1 && (
                      <span className="ml-1 text-error">no disponible</span>
                    )}
                    <div className="text-[10px] text-muted">
                      {formatBytes(f.size_bytes)}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={f.is_available !== 1}
                    onClick={async () => {
                      try {
                        setVersionError(null);
                        await api.openDocumentFile(
                          f.id,
                          f.relative_path.includes(".ingmanager/snapshots"),
                        );
                      } catch (e) {
                        setVersionError(String(e));
                      }
                    }}
                  >
                    Abrir
                  </Button>
                </li>
              ))}
            </ul>
            {versionError && inspectorTab === "doc-versions" && (
              <div className="border border-error/40 bg-error/10 px-2 py-1.5 text-[12px] text-error">
                {versionError}
              </div>
            )}

            {activeVersionId &&
              versions.data?.find((v) => v.id === activeVersionId)?.is_current !==
                1 && (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={async () => {
                    await api.restoreDocumentVersion(
                      documentId,
                      activeVersionId,
                      "Restauración como nueva versión",
                    );
                    await qc.invalidateQueries({ queryKey: ["document", documentId] });
                    await qc.invalidateQueries({ queryKey: ["documents"] });
                    await qc.invalidateQueries({ queryKey: ["doc-versions", documentId] });
                    setSelectedVersionId(null);
                  }}
                >
                  Restaurar como nueva versión
                </Button>
              )}

            {isPlanosOrMemorias && (
              <>
                <div className="pt-2 text-[11px] font-semibold uppercase text-muted">
                  Revisiones emitidas
                </div>
                {(revisions.data?.length ?? 0) === 0 && (
                  <p className="text-[12px] text-muted">Sin revisiones emitidas.</p>
                )}
                <ul className="space-y-1">
                  {revisions.data?.map((r) => (
                    <li
                      key={r.id}
                      className="border border-border/60 px-2 py-1.5 text-[12px]"
                    >
                      <div className="font-mono text-accent">{r.revision_code}</div>
                      <div className="text-[11px] text-muted">
                        {r.issued_at
                          ? new Date(r.issued_at).toLocaleDateString()
                          : "sin fecha"}{" "}
                        · {r.status ?? "issued"}
                      </div>
                      {r.notes && <div className="text-muted">{r.notes}</div>}
                    </li>
                  ))}
                </ul>
                <Button
                  size="sm"
                  onClick={async () => {
                    const code = window.prompt("Código de revisión emitida (ej. A, 0, R1)");
                    if (!code) return;
                    const picked = await open({
                      multiple: true,
                      title: "Formatos de la revisión (PDF, DWG, …)",
                    });
                    const paths = !picked
                      ? []
                      : Array.isArray(picked)
                        ? picked
                        : [picked];
                    await api.createRevision({
                      document_id: documentId,
                      revision_code: code,
                      issued_at: new Date().toISOString().slice(0, 10),
                      document_version_id: d.current_version_id,
                      file_paths: paths,
                      format_labels: paths.map((p) => {
                        const ext = p.split(".").pop()?.toUpperCase();
                        return ext ?? "FILE";
                      }),
                    });
                    await qc.invalidateQueries({
                      queryKey: ["doc-revisions", documentId],
                    });
                    await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
                  }}
                >
                  Emitir revisión…
                </Button>
              </>
            )}
          </>
        )}

        {inspectorTab === "doc-related" && (
          <>
            <div className="space-y-2 border border-border bg-elevated p-2">
              <div className="text-[11px] font-semibold uppercase text-muted">
                Nueva relación
              </div>
              <label className="block text-[12px]">
                <span className="mb-1 block text-muted">Documento destino</span>
                <select
                  className="h-[32px] w-full border border-border bg-panel px-2"
                  value={relTarget}
                  onChange={(e) => setRelTarget(e.target.value)}
                >
                  <option value="">Seleccionar…</option>
                  {projectDocs.data
                    ?.filter((x) => x.id !== documentId)
                    .map((x) => (
                      <option key={x.id} value={x.id}>
                        {x.business_code ?? "—"} · {x.title}
                      </option>
                    ))}
                </select>
              </label>
              <label className="block text-[12px]">
                <span className="mb-1 block text-muted">Tipo</span>
                <select
                  className="h-[32px] w-full border border-border bg-panel px-2"
                  value={relType}
                  onChange={(e) => setRelType(e.target.value)}
                >
                  {RELATION_TYPES.map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </select>
              </label>
              <label className="block text-[12px]">
                <span className="mb-1 block text-muted">Notas</span>
                <input
                  className="h-[32px] w-full border border-border bg-panel px-2"
                  value={relNotes}
                  onChange={(e) => setRelNotes(e.target.value)}
                />
              </label>
              <Button
                size="sm"
                disabled={!relTarget || createRelMut.isPending}
                onClick={() => createRelMut.mutate()}
              >
                Añadir relación
              </Button>
              {relError && (
                <p className="text-[12px] text-error">{relError}</p>
              )}
            </div>

            {(related.data?.length ?? 0) === 0 && (
              <p className="text-[12px] text-muted">Sin relaciones registradas.</p>
            )}
            <ul className="space-y-1">
              {related.data?.map((r) => {
                const otherId =
                  r.source_document_id === documentId
                    ? r.target_document_id
                    : r.source_document_id;
                const otherTitle =
                  r.source_document_id === documentId
                    ? r.target_title
                    : r.source_title;
                const otherCode =
                  r.source_document_id === documentId
                    ? r.target_code
                    : r.source_code;
                const otherVer =
                  r.source_document_id === documentId
                    ? r.target_version_label
                    : r.source_version_label;
                return (
                  <li
                    key={r.id}
                    className="flex items-start justify-between gap-2 border border-border/60 px-2 py-1.5 text-[12px]"
                  >
                    <button
                      type="button"
                      className="min-w-0 flex-1 text-left hover:text-accent"
                      onClick={() => {
                        setSelectedDocumentId(otherId);
                        setInspectorTab("doc-detail");
                      }}
                    >
                      <div className="font-mono text-accent">{r.relation_type}</div>
                      <div className="truncate">
                        {otherCode ?? "—"} · {otherTitle ?? otherId.slice(0, 8)}
                        {otherVer ? ` · ${otherVer}` : ""}
                      </div>
                      {r.notes && (
                        <div className="text-[11px] text-muted">{r.notes}</div>
                      )}
                    </button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={async () => {
                        await api.deleteDocumentRelationship(r.id);
                        await qc.invalidateQueries({
                          queryKey: ["doc-related", documentId],
                        });
                      }}
                    >
                      Quitar
                    </Button>
                  </li>
                );
              })}
            </ul>
          </>
        )}
      </div>
    </div>
  );
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-[10.5px] font-medium uppercase tracking-wide text-muted">
        {label}
      </div>
      <div className="text-[12.5px]">{value}</div>
    </div>
  );
}
