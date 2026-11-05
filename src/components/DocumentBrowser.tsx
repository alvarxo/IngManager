import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  FilePlus2,
  Plus,
} from "lucide-react";
import { api } from "@/lib/api";
import {
  categoryUsesRevisions,
  expandedSetToRecord,
  formatChipLabel,
  initialExpandedKeys,
  treeNodeKey,
} from "@/lib/logical-tree";
import {
  filterDocsByModelType,
  groupDocsByModelType,
  MODEL_TYPE_OPTIONS,
  modelTypeLabel,
} from "@/lib/model-types";
import type {
  DocumentFileRow,
  DocumentRow,
  DocumentVersionRow,
  ModelType,
  RevisionRow,
} from "@/lib/types";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";
import { CreateDerivedModelDialog } from "./CreateDerivedModelDialog";
import { RepairModelHistoryDialog } from "./RepairModelHistoryDialog";
import { UnifyModelsDialog } from "./UnifyModelsDialog";

interface Props {
  projectId: string;
  categoryKey: string;
}

const MEMORIA_STATUSES = ["desarrollo", "revision", "aprobada", "obsoleta"];

type CtxTarget =
  | { kind: "doc"; doc: DocumentRow }
  | { kind: "file"; doc: DocumentRow; file: DocumentFileRow };

interface DocContextMenu {
  x: number;
  y: number;
  target: CtxTarget;
  formats: string[];
}

interface FormatPickerState {
  documentId: string;
  formats: string[];
  rememberOnChoose: boolean;
  title: string;
}

export function DocumentBrowser({ projectId, categoryKey }: Props) {
  const qc = useQueryClient();
  const selectedDocumentId = useUiStore((s) => s.selectedDocumentId);
  const setSelectedDocumentId = useUiStore((s) => s.setSelectedDocumentId);
  const setInspectorTab = useUiStore((s) => s.setInspectorTab);
  const showCurrentVersionOnly = useUiStore((s) => s.showCurrentVersionOnly);
  const setShowCurrentVersionOnly = useUiStore((s) => s.setShowCurrentVersionOnly);

  const [createOpen, setCreateOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [status, setStatus] = useState(
    categoryKey === "memorias" ? "desarrollo" : "active",
  );
  const [subcategory, setSubcategory] = useState("");
  const [origin, setOrigin] = useState("");
  const [receivedAt, setReceivedAt] = useState("");
  const [assignCode, setAssignCode] = useState(categoryKey !== "antecedentes");
  const [sourcePath, setSourcePath] = useState<string | null>(null);
  const [createModelType, setCreateModelType] = useState<ModelType>("other");
  const [modelTypeFilter, setModelTypeFilter] = useState<ModelType | "all">(
    "all",
  );
  const [groupByModelType, setGroupByModelType] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ctxMenu, setCtxMenu] = useState<DocContextMenu | null>(null);
  const [unifyOpen, setUnifyOpen] = useState(false);
  const [unifySuggestedDups, setUnifySuggestedDups] = useState<string[]>([]);
  const [derivedSource, setDerivedSource] = useState<DocumentRow | null>(null);
  const [repairHistoryOpen, setRepairHistoryOpen] = useState(false);
  const [formatPicker, setFormatPicker] = useState<FormatPickerState | null>(
    null,
  );
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [expandReady, setExpandReady] = useState(false);
  const persistTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const usesRevisions = categoryUsesRevisions(categoryKey);

  const docs = useQuery({
    queryKey: ["documents", projectId, categoryKey],
    queryFn: () => api.listDocuments(projectId, categoryKey, true),
  });

  const expandPolicy = useQuery({
    queryKey: ["logical-tree-expand-policy"],
    queryFn: () => api.getLogicalTreeExpandPolicy(),
  });

  const expandState = useQuery({
    queryKey: ["logical-tree-expand-state", projectId],
    queryFn: () => api.getLogicalTreeExpandState(projectId),
  });

  useEffect(() => {
    if (!expandPolicy.data || expandState.isLoading) return;
    const seed =
      expandPolicy.data === "expanded"
        ? (docs.data ?? []).map((d) => treeNodeKey("doc", d.id))
        : [];
    setExpanded(
      initialExpandedKeys(expandPolicy.data, expandState.data ?? {}, seed),
    );
    setExpandReady(true);
  }, [expandPolicy.data, expandState.data, expandState.isLoading, docs.data]);

  const persistExpand = useCallback(
    (next: Set<string>) => {
      if (expandPolicy.data !== "remember") return;
      if (persistTimer.current) clearTimeout(persistTimer.current);
      persistTimer.current = setTimeout(() => {
        void api
          .setLogicalTreeExpandState(projectId, expandedSetToRecord(next))
          .then(() =>
            qc.invalidateQueries({
              queryKey: ["logical-tree-expand-state", projectId],
            }),
          )
          .catch((e) => setError(String(e)));
      }, 250);
    },
    [expandPolicy.data, projectId, qc],
  );

  function toggleExpand(key: string, e?: ReactMouseEvent) {
    e?.preventDefault();
    e?.stopPropagation();
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      persistExpand(next);
      return next;
    });
  }

  const codePreview = useQuery({
    queryKey: [
      "doc-code",
      projectId,
      categoryKey,
      assignCode,
      categoryKey === "modelos" ? createModelType : null,
    ],
    queryFn: () =>
      api.previewNextDocumentCode(
        projectId,
        categoryKey,
        categoryKey === "modelos" ? createModelType : null,
      ),
    enabled: createOpen && assignCode,
  });

  const visibleDocs = useMemo(() => {
    const list = docs.data ?? [];
    if (categoryKey !== "modelos") return list;
    return filterDocsByModelType(list, modelTypeFilter);
  }, [docs.data, categoryKey, modelTypeFilter]);

  const groupedDocs = useMemo(() => {
    if (categoryKey !== "modelos" || !groupByModelType) return null;
    return groupDocsByModelType(visibleDocs);
  }, [categoryKey, groupByModelType, visibleDocs]);

  const subcats = useQuery({
    queryKey: ["subcategories", projectId, categoryKey],
    queryFn: () => api.listSubcategories(projectId, categoryKey),
    enabled: categoryKey === "antecedentes",
  });

  const createMut = useMutation({
    mutationFn: () =>
      api.createDocument({
        project_id: projectId,
        category_key: categoryKey,
        title,
        status,
        subcategory: subcategory || null,
        origin: origin || null,
        received_at: receivedAt || null,
        assign_code: assignCode,
        source_absolute_path: sourcePath,
        model_type: categoryKey === "modelos" ? createModelType : null,
      }),
    onSuccess: async (doc) => {
      await qc.invalidateQueries({ queryKey: ["documents", projectId, categoryKey] });
      setCreateOpen(false);
      setTitle("");
      setSourcePath(null);
      setCreateModelType("other");
      setSelectedDocumentId(doc.id);
      setInspectorTab("doc-detail");
    },
    onError: (e: Error) => setError(String(e)),
  });

  const bulkReg = useMutation({
    mutationFn: () =>
      api.bulkRegisterEngineeringDocs({
        project_id: projectId,
        category_key: categoryKey,
        assign_codes: true,
      }),
    onSuccess: async (r) => {
      await qc.invalidateQueries({ queryKey: ["documents", projectId, categoryKey] });
      await qc.invalidateQueries({ queryKey: ["categories", projectId] });
      await qc.invalidateQueries({ queryKey: ["doc-versions"] });
      await qc.invalidateQueries({ queryKey: ["document"] });
      setError(null);
      window.alert(
        `Registrados ${r.created} modelos (sin copiar). Versiones añadidas a existentes: ${r.versions_added}. Omitidos: ${r.skipped}.`,
      );
    },
    onError: (e: Error) => setError(String(e)),
  });

  const syncVersionsMut = useMutation({
    mutationFn: () =>
      api.syncModelVersions({
        project_id: projectId,
        category_key: categoryKey,
        document_id: selectedDocumentId,
      }),
    onSuccess: async (r) => {
      await qc.invalidateQueries({ queryKey: ["documents", projectId, categoryKey] });
      await qc.invalidateQueries({ queryKey: ["doc-versions"] });
      await qc.invalidateQueries({ queryKey: ["document"] });
      await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
      setError(null);
      let msg = `Sincronización: ${r.versions_added} versión(es) añadidas en ${r.documents_touched} documento(s). Ya vinculadas: ${r.already_linked}.`;
      const suggestions = r.unify_suggestions ?? [];
      if (suggestions.length > 0) {
        const s = suggestions[0];
        msg += `\n\nLa familia «${s.group_key}» está partida entre ${s.document_labels.join(", ")}. ¿Abrir Unificar modelos…?`;
        if (window.confirm(msg)) {
          const principal =
            selectedDocumentId && s.document_ids.includes(selectedDocumentId)
              ? selectedDocumentId
              : s.document_ids[0];
          setUnifySuggestedDups(s.document_ids.filter((id) => id !== principal));
          if (principal) setSelectedDocumentId(principal);
          setUnifyOpen(true);
          return;
        }
      } else {
        window.alert(msg);
      }
    },
    onError: (e: Error) => setError(String(e)),
  });

  const cleanupMut = useMutation({
    mutationFn: () => api.cleanupDocumentAvailability(projectId, true),
    onSuccess: async (r) => {
      await qc.invalidateQueries({ queryKey: ["documents", projectId, categoryKey] });
      await qc.invalidateQueries({ queryKey: ["categories", projectId] });
      await qc.invalidateQueries({ queryKey: ["document"] });
      await qc.invalidateQueries({ queryKey: ["doc-versions"] });
      await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
      setError(null);
      if (selectedDocumentId && r.document_ids_removed.includes(selectedDocumentId)) {
        setSelectedDocumentId(null);
      }
      window.alert(
        `Limpieza: ${r.marked_unavailable} archivo(s) marcados no disponible; ${r.deleted_empty_documents} documento(s) lógico(s) vacío(s) eliminados (archivos físicos conservados).`,
      );
    },
    onError: (e: Error) => setError(String(e)),
  });

  async function deleteLogical(doc: DocumentRow) {
    const label = doc.business_code ?? doc.title;
    if (
      !window.confirm(
        `¿Eliminar documento lógico «${label}»?\n\nSolo se borra el registro en IngManager (SQLite). Los archivos físicos se conservan.`,
      )
    ) {
      return;
    }
    try {
      setError(null);
      await api.deleteLogicalDocument(doc.id);
      if (selectedDocumentId === doc.id) setSelectedDocumentId(null);
      await qc.invalidateQueries({ queryKey: ["documents", projectId, categoryKey] });
      await qc.invalidateQueries({ queryKey: ["categories", projectId] });
      await qc.invalidateQueries({ queryKey: ["document", doc.id] });
      await qc.invalidateQueries({ queryKey: ["doc-versions", doc.id] });
      setCtxMenu(null);
    } catch (e) {
      setError(String(e));
      setCtxMenu(null);
    }
  }

  async function openDocPreferred(docId: string) {
    try {
      setError(null);
      setSelectedDocumentId(docId);
      setInspectorTab("doc-detail");
      const formats = await api.listDocumentOpenFormats(docId);
      if (formats.needs_picker) {
        setFormatPicker({
          documentId: docId,
          formats: formats.formats,
          rememberOnChoose: true,
          title: "Elegir formato preferido",
        });
        return;
      }
      await api.openDocumentPrincipal(docId);
    } catch (e) {
      setError(String(e));
    }
  }

  async function openFormatOnce(docId: string, format: string) {
    try {
      setError(null);
      await api.openDocumentWithFormat(docId, format, false);
      setCtxMenu(null);
    } catch (e) {
      setError(String(e));
      setCtxMenu(null);
    }
  }

  async function changeDefaultFormat(docId: string, format: string) {
    try {
      setError(null);
      await api.setDocumentOpenPreference(docId, format);
      setCtxMenu(null);
    } catch (e) {
      setError(String(e));
      setCtxMenu(null);
    }
  }

  async function openFileLeaf(file: DocumentFileRow) {
    try {
      setError(null);
      await api.openDocumentFile(
        file.id,
        file.relative_path.includes(".ingmanager/snapshots"),
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function onDocContextMenu(e: ReactMouseEvent, doc: DocumentRow) {
    e.preventDefault();
    e.stopPropagation();
    setSelectedDocumentId(doc.id);
    let formats: string[] = [];
    try {
      formats = (await api.listDocumentOpenFormats(doc.id)).formats;
    } catch {
      formats = [];
    }
    setCtxMenu({
      x: e.clientX,
      y: e.clientY,
      target: { kind: "doc", doc },
      formats,
    });
  }

  async function associateSheetToDoc(
    doc: DocumentRow,
    sheetKind: "calc" | "export",
  ) {
    try {
      setError(null);
      if (!doc.current_version_id) {
        setError("El documento no tiene versión actual");
        return;
      }
      const picked = await open({
        multiple: true,
        title:
          sheetKind === "calc"
            ? "Asociar planilla(s) calc (XLSX/XLS/XLSM)"
            : "Asociar planilla(s) export (XLSX/XLS/XLSM)",
        filters: [
          {
            name: "Excel",
            extensions: ["xlsx", "xls", "xlsm"],
          },
        ],
      });
      const paths = Array.isArray(picked)
        ? picked
        : typeof picked === "string"
          ? [picked]
          : [];
      if (paths.length === 0) return;
      for (const absolute_path of paths) {
        await api.associateModelSpreadsheet({
          document_version_id: doc.current_version_id,
          absolute_path,
          sheet_kind: sheetKind,
        });
      }
      await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
      await qc.invalidateQueries({ queryKey: ["doc-versions", doc.id] });
      await qc.invalidateQueries({ queryKey: ["documents", projectId, categoryKey] });
      await qc.invalidateQueries({ queryKey: ["model-sheet-status"] });
    } catch (e) {
      setError(String(e));
    }
  }

  async function detachSheet(file: DocumentFileRow) {
    try {
      setError(null);
      if (
        !window.confirm(
          `¿Quitar la asociación de «${file.file_name}»? El archivo en disco no se borra.`,
        )
      ) {
        return;
      }
      await api.detachModelSpreadsheet(file.id);
      await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
    } catch (e) {
      setError(String(e));
    }
  }

  const statuses = useMemo(() => {
    if (categoryKey === "memorias") return MEMORIA_STATUSES;
    return ["active", "archived", "obsoleta"];
  }, [categoryKey]);

  async function pickSource() {
    const picked = await open({
      multiple: false,
      directory: false,
      title: "Asociar archivo al documento (copia a snapshot)",
    });
    if (typeof picked === "string") setSourcePath(picked);
  }

  async function addSubcategory() {
    const name = window.prompt("Nueva subcategoría de antecedentes");
    if (!name) return;
    await api.createSubcategory(projectId, categoryKey, name);
    await qc.invalidateQueries({
      queryKey: ["subcategories", projectId, categoryKey],
    });
  }

  async function renameSubcategory(id: string, current: string) {
    const name = window.prompt("Renombrar subcategoría", current);
    if (!name || name.trim() === current) return;
    try {
      await api.updateSubcategory(id, name);
      await qc.invalidateQueries({
        queryKey: ["subcategories", projectId, categoryKey],
      });
      await qc.invalidateQueries({ queryKey: ["documents", projectId, categoryKey] });
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeSubcategory(id: string, name: string) {
    if (!window.confirm(`¿Eliminar subcategoría «${name}»? Los documentos no se borran.`)) {
      return;
    }
    try {
      await api.deleteSubcategory(id);
      await qc.invalidateQueries({
        queryKey: ["subcategories", projectId, categoryKey],
      });
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-border bg-panel px-3 py-1.5">
        <Button size="sm" onClick={() => setCreateOpen(true)}>
          <Plus size={14} /> Nuevo documento
        </Button>
        {categoryKey === "modelos" && (
          <>
            <Button
              size="sm"
              variant="secondary"
              disabled={bulkReg.isPending}
              onClick={() => bulkReg.mutate()}
              title="Registrar o sincronizar modelos (.sdb): crea documentos nuevos y añade versiones faltantes a los ya registrados (sin copiar)"
            >
              <FilePlus2 size={14} /> Registrar / sincronizar…
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={syncVersionsMut.isPending || !selectedDocumentId}
              onClick={() => syncVersionsMut.mutate()}
              title="Sincronizar versiones físicas faltantes del documento seleccionado (p.ej. v01–v03 → 1126-MOD-001)"
            >
              Sync versiones
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={!selectedDocumentId}
              onClick={() => {
                const doc = (docs.data ?? []).find(
                  (d) => d.id === selectedDocumentId,
                );
                if (doc) setDerivedSource(doc);
              }}
              title="Copiar el .sdb actual como nuevo modelo (UUID + MOD nuevos); opcionalmente copia planillas calc a paths nuevos; provenance deriva_de"
            >
              Crear derivado…
            </Button>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                setUnifySuggestedDups([]);
                setUnifyOpen(true);
              }}
              title="Fusionar documentos lógicos duplicados de la misma familia técnica (conserva UUID/código/título del principal; no borra .sdb)"
            >
              Unificar modelos…
            </Button>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => setRepairHistoryOpen(true)}
              title="Quitar versiones falsas (.sbk/aux) del historial; conserva solo .sdb. Preview + backup SQLite. No borra archivos físicos."
            >
              Reparar historial…
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={cleanupMut.isPending}
              onClick={() => cleanupMut.mutate()}
              title="Marcar archivos faltantes como no disponible y quitar documentos lógicos vacíos (sin borrar archivos)"
            >
              Limpiar fantasmas…
            </Button>
          </>
        )}
        {categoryKey === "modelos" && (
          <>
            <label className="flex items-center gap-1.5 text-[12px] text-muted">
              Tipo
              <select
                className="h-[28px] border border-border bg-elevated px-1.5 text-[12px] text-text"
                value={modelTypeFilter}
                onChange={(e) =>
                  setModelTypeFilter(e.target.value as ModelType | "all")
                }
                title="Filtrar por subtipo de modelo"
              >
                {MODEL_TYPE_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-2 text-[12px] text-muted">
              <input
                type="checkbox"
                checked={groupByModelType}
                onChange={(e) => setGroupByModelType(e.target.checked)}
              />
              Agrupar por tipo
            </label>
          </>
        )}
        <label className="ml-auto flex items-center gap-2 text-[12px] text-muted">
          <input
            type="checkbox"
            checked={showCurrentVersionOnly}
            onChange={(e) => setShowCurrentVersionOnly(e.target.checked)}
          />
          Mostrar solo versión actual
        </label>
        {categoryKey === "antecedentes" && (
          <Button variant="secondary" size="sm" onClick={addSubcategory}>
            Subcategoría…
          </Button>
        )}
      </div>
      {categoryKey === "antecedentes" && (subcats.data?.length ?? 0) > 0 && (
        <div className="flex flex-wrap items-center gap-1 border-b border-border bg-panel px-3 py-1">
          <span className="mr-1 text-[11px] uppercase text-muted">Subcats:</span>
          {subcats.data?.map((s) => (
            <span
              key={s.id}
              className="inline-flex items-center gap-1 border border-border bg-elevated px-1.5 py-0.5 text-[11.5px]"
            >
              {s.name}
              <button
                type="button"
                className="text-muted hover:text-text"
                title="Renombrar"
                onClick={() => renameSubcategory(s.id, s.name)}
              >
                ✎
              </button>
              <button
                type="button"
                className="text-muted hover:text-error"
                title="Eliminar"
                onClick={() => removeSubcategory(s.id, s.name)}
              >
                ×
              </button>
            </span>
          ))}
        </div>
      )}

      <div
        className="min-h-0 flex-1 overflow-auto"
        onClick={() => setCtxMenu(null)}
      >
        {docs.isLoading && <div className="p-4 text-muted">Cargando documentos…</div>}
        {docs.error && (
          <div className="p-4 text-error">{String(docs.error)}</div>
        )}
        {error && (
          <div className="m-3 border border-error/40 bg-error/10 px-3 py-2 text-[12.5px] text-error">
            {error}
          </div>
        )}
        {!docs.isLoading && (docs.data?.length ?? 0) === 0 && (
          <div className="p-8 text-center text-muted">
            No hay documentos lógicos en esta categoría.
            <div className="mt-1 text-[11.5px]">
              Cree un documento (código PROYECTO-TIPO-CORRELATIVO) sin obligar a
              renombrar archivos físicos.
            </div>
          </div>
        )}
        {!docs.isLoading &&
          (docs.data?.length ?? 0) > 0 &&
          visibleDocs.length === 0 && (
            <div className="p-8 text-center text-muted">
              No hay modelos para el filtro de tipo seleccionado.
            </div>
          )}
        {visibleDocs.length > 0 && expandReady && (
          <table className="w-full border-collapse text-left">
            <thead className="sticky top-0 bg-panel">
              <tr className="border-b border-border text-[11px] uppercase text-muted">
                <th className="h-[36px] px-3 font-medium">Documento / árbol</th>
                <th className="w-[100px] px-3 font-medium">Estado</th>
                {usesRevisions && (
                  <th className="w-[100px] px-3 font-medium">Revisión actual</th>
                )}
                <th className="w-[70px] px-3 font-medium">Versión</th>
                <th className="w-[70px] px-3 font-medium">Archivos</th>
                <th className="w-[110px] px-3 font-medium">Notas</th>
              </tr>
            </thead>
            <tbody>
              {(groupedDocs
                ? groupedDocs.flatMap((g) => [
                    {
                      kind: "header" as const,
                      key: `hdr-${g.modelType}`,
                      label: `${g.label} (${g.docs.length})`,
                    },
                    ...g.docs.map((d) => ({
                      kind: "doc" as const,
                      key: d.id,
                      doc: d,
                    })),
                  ])
                : visibleDocs.map((d) => ({
                    kind: "doc" as const,
                    key: d.id,
                    doc: d,
                  }))
              ).map((row) =>
                row.kind === "header" ? (
                  <tr
                    key={row.key}
                    className="border-b border-border bg-elevated/60"
                  >
                    <td
                      colSpan={usesRevisions ? 6 : 5}
                      className="px-3 py-1.5 text-[11px] font-medium uppercase tracking-wide text-muted"
                    >
                      {row.label}
                    </td>
                  </tr>
                ) : (
                  <DocumentTreeRows
                    key={row.key}
                    doc={row.doc}
                    projectId={projectId}
                    categoryKey={categoryKey}
                    usesRevisions={usesRevisions}
                    showCurrentVersionOnly={showCurrentVersionOnly}
                    expanded={expanded}
                    selectedDocumentId={selectedDocumentId}
                    onToggleExpand={toggleExpand}
                    onSelectDoc={(id) => {
                      setSelectedDocumentId(id);
                      setInspectorTab("doc-detail");
                      setCtxMenu(null);
                    }}
                    onOpenDoc={(id) => void openDocPreferred(id)}
                    onOpenFile={(f) => void openFileLeaf(f)}
                    onDocContextMenu={onDocContextMenu}
                    onFileContextMenu={(e, doc, file) => {
                      e.preventDefault();
                      e.stopPropagation();
                      setSelectedDocumentId(doc.id);
                      setCtxMenu({
                        x: e.clientX,
                        y: e.clientY,
                        target: { kind: "file", doc, file },
                        formats: [],
                      });
                    }}
                  />
                ),
              )}
            </tbody>
          </table>
        )}
        {ctxMenu?.target.kind === "doc" ? (
          <div
            className="fixed z-50 max-h-[70vh] min-w-[240px] overflow-auto border border-border bg-panel py-1 shadow-lg"
            style={{ left: ctxMenu.x, top: ctxMenu.y }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              type="button"
              className="block w-full px-3 py-1.5 text-left text-[12.5px] hover:bg-elevated"
              onClick={() => {
                const id = (ctxMenu.target as { kind: "doc"; doc: DocumentRow }).doc
                  .id;
                setSelectedDocumentId(id);
                setInspectorTab("doc-detail");
                setCtxMenu(null);
              }}
            >
              Abrir detalle
            </button>
            <button
              type="button"
              className="block w-full px-3 py-1.5 text-left text-[12.5px] hover:bg-elevated"
              onClick={() => {
                const id = (ctxMenu.target as { kind: "doc"; doc: DocumentRow }).doc
                  .id;
                setCtxMenu(null);
                void openDocPreferred(id);
              }}
            >
              Abrir (preferido / actual)
            </button>
            {ctxMenu.formats.map((fmt) => (
              <button
                key={`once-${fmt}`}
                type="button"
                className="block w-full px-3 py-1.5 text-left text-[12.5px] hover:bg-elevated"
                onClick={() => {
                  const id = (ctxMenu.target as { kind: "doc"; doc: DocumentRow })
                    .doc.id;
                  void openFormatOnce(id, fmt);
                }}
              >
                Abrir {formatChipLabel(fmt)} (sin cambiar predeterminado)
              </button>
            ))}
            {ctxMenu.formats.length > 0 && (
              <div className="my-1 border-t border-border" />
            )}
            {ctxMenu.formats.map((fmt) => (
              <button
                key={`def-${fmt}`}
                type="button"
                className="block w-full px-3 py-1.5 text-left text-[12.5px] hover:bg-elevated"
                onClick={() => {
                  const id = (ctxMenu.target as { kind: "doc"; doc: DocumentRow })
                    .doc.id;
                  void changeDefaultFormat(id, fmt);
                }}
              >
                Predeterminado: {formatChipLabel(fmt)}
              </button>
            ))}
            {categoryKey === "modelos" && (
              <>
                <div className="my-1 border-t border-border" />
                <button
                  type="button"
                  className="block w-full px-3 py-1.5 text-left text-[12.5px] hover:bg-elevated"
                  onClick={() => {
                    const d = (ctxMenu.target as { kind: "doc"; doc: DocumentRow })
                      .doc;
                    setCtxMenu(null);
                    void associateSheetToDoc(d, "calc");
                  }}
                >
                  Asociar planilla calc…
                </button>
                <button
                  type="button"
                  className="block w-full px-3 py-1.5 text-left text-[12.5px] hover:bg-elevated"
                  onClick={() => {
                    const d = (ctxMenu.target as { kind: "doc"; doc: DocumentRow })
                      .doc;
                    setCtxMenu(null);
                    void associateSheetToDoc(d, "export");
                  }}
                >
                  Asociar planilla export…
                </button>
                <button
                  type="button"
                  className="block w-full px-3 py-1.5 text-left text-[12.5px] hover:bg-elevated"
                  onClick={() => {
                    const d = (ctxMenu.target as { kind: "doc"; doc: DocumentRow })
                      .doc;
                    setCtxMenu(null);
                    setDerivedSource(d);
                  }}
                >
                  Crear modelo derivado…
                </button>
                <button
                  type="button"
                  className="block w-full px-3 py-1.5 text-left text-[12.5px] hover:bg-elevated"
                  onClick={() => {
                    const d = (ctxMenu.target as { kind: "doc"; doc: DocumentRow })
                      .doc;
                    setSelectedDocumentId(d.id);
                    setUnifySuggestedDups([]);
                    setCtxMenu(null);
                    setUnifyOpen(true);
                  }}
                >
                  Unificar modelos…
                </button>
              </>
            )}
            <div className="my-1 border-t border-border" />
            <button
              type="button"
              className="block w-full px-3 py-1.5 text-left text-[12.5px] text-error hover:bg-elevated"
              onClick={() => {
                const d = (ctxMenu.target as { kind: "doc"; doc: DocumentRow }).doc;
                void deleteLogical(d);
              }}
            >
              Eliminar documento lógico…
            </button>
          </div>
        ) : null}
        {ctxMenu?.target.kind === "file" ? (
          <div
            className="fixed z-50 min-w-[220px] border border-border bg-panel py-1 shadow-lg"
            style={{ left: ctxMenu.x, top: ctxMenu.y }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              type="button"
              className="block w-full px-3 py-1.5 text-left text-[12.5px] hover:bg-elevated"
              onClick={() => {
                const f = (
                  ctxMenu.target as { kind: "file"; file: DocumentFileRow }
                ).file;
                setCtxMenu(null);
                void openFileLeaf(f);
              }}
            >
              Abrir este archivo
              {(
                ctxMenu.target as { kind: "file"; file: DocumentFileRow }
              ).file.relative_path.includes(".ingmanager/snapshots/")
                ? " (copia segura)"
                : ""}
            </button>
            {(
              ctxMenu.target as { kind: "file"; file: DocumentFileRow }
            ).file.sheet_kind && (
              <button
                type="button"
                className="block w-full px-3 py-1.5 text-left text-[12.5px] text-error hover:bg-elevated"
                onClick={() => {
                  const f = (
                    ctxMenu.target as { kind: "file"; file: DocumentFileRow }
                  ).file;
                  setCtxMenu(null);
                  void detachSheet(f);
                }}
              >
                Quitar asociación de planilla…
              </button>
            )}
          </div>
        ) : null}
      </div>

      {formatPicker && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55">
          <div className="w-[360px] border border-border bg-panel shadow-xl">
            <div className="border-b border-border px-4 py-3">
              <h2 className="text-[14px] font-semibold">{formatPicker.title}</h2>
              <p className="text-[12px] text-muted">
                Se recordará por documento para el próximo doble clic.
              </p>
            </div>
            <div className="flex flex-wrap gap-2 p-4">
              {formatPicker.formats.map((fmt) => (
                <Button
                  key={fmt}
                  size="sm"
                  onClick={() => {
                    const id = formatPicker.documentId;
                    const remember = formatPicker.rememberOnChoose;
                    setFormatPicker(null);
                    void (async () => {
                      try {
                        await api.openDocumentWithFormat(id, fmt, remember);
                      } catch (e) {
                        setError(String(e));
                      }
                    })();
                  }}
                >
                  {formatChipLabel(fmt)}
                </Button>
              ))}
            </div>
            <div className="flex justify-end border-t border-border px-4 py-3">
              <Button
                variant="secondary"
                onClick={() => setFormatPicker(null)}
              >
                Cancelar
              </Button>
            </div>
          </div>
        </div>
      )}

      {createOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55">
          <div className="w-[480px] border border-border bg-panel shadow-xl">
            <div className="border-b border-border px-4 py-3">
              <h2 className="text-[14px] font-semibold">Nuevo documento</h2>
              <p className="text-[12px] text-muted">
                Categoría {categoryKey} · UUID interno · código de negocio aparte
              </p>
            </div>
            <div className="space-y-3 p-4">
              {(categoryKey !== "antecedentes" || assignCode) && (
                <label className="block">
                  <div className="mb-1 text-[11.5px] text-muted">Código</div>
                  <div className="h-[34px] border border-border bg-elevated px-2.5 py-1.5 font-mono text-accent">
                    {assignCode ? (codePreview.data ?? "····") : "Sin código"}
                  </div>
                </label>
              )}
              {categoryKey === "modelos" && (
                <label className="block">
                  <div className="mb-1 text-[11.5px] text-muted">
                    Tipo de modelo
                  </div>
                  <select
                    className="h-[34px] w-full border border-border bg-elevated px-2.5"
                    value={createModelType}
                    onChange={(e) =>
                      setCreateModelType(e.target.value as ModelType)
                    }
                  >
                    {MODEL_TYPE_OPTIONS.filter((o) => o.value !== "all").map(
                      (o) => (
                        <option key={o.value} value={o.value}>
                          {o.label} ({o.prefix})
                        </option>
                      ),
                    )}
                  </select>
                </label>
              )}
              {categoryKey === "antecedentes" && (
                <label className="flex items-center gap-2 text-[12.5px]">
                  <input
                    type="checkbox"
                    checked={assignCode}
                    onChange={(e) => setAssignCode(e.target.checked)}
                  />
                  Asignar código ANT
                </label>
              )}
              <label className="block">
                <div className="mb-1 text-[11.5px] text-muted">Título *</div>
                <input
                  className="h-[34px] w-full border border-border bg-elevated px-2.5"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  autoFocus
                />
              </label>
              <label className="block">
                <div className="mb-1 text-[11.5px] text-muted">Estado</div>
                <select
                  className="h-[34px] w-full border border-border bg-elevated px-2.5"
                  value={status}
                  onChange={(e) => setStatus(e.target.value)}
                >
                  {statuses.map((s) => (
                    <option key={s} value={s}>
                      {s}
                    </option>
                  ))}
                </select>
              </label>
              {categoryKey === "antecedentes" && (
                <>
                  <label className="block">
                    <div className="mb-1 text-[11.5px] text-muted">Subcategoría</div>
                    <input
                      className="h-[34px] w-full border border-border bg-elevated px-2.5"
                      list="subcat-list"
                      value={subcategory}
                      onChange={(e) => setSubcategory(e.target.value)}
                      placeholder="Opcional"
                    />
                    <datalist id="subcat-list">
                      {subcats.data?.map((s) => (
                        <option key={s.id} value={s.name} />
                      ))}
                    </datalist>
                  </label>
                  <label className="block">
                    <div className="mb-1 text-[11.5px] text-muted">Origen / recepción</div>
                    <input
                      className="h-[34px] w-full border border-border bg-elevated px-2.5"
                      value={origin}
                      onChange={(e) => setOrigin(e.target.value)}
                    />
                  </label>
                  <label className="block">
                    <div className="mb-1 text-[11.5px] text-muted">Fecha recepción</div>
                    <input
                      type="date"
                      className="h-[34px] w-full border border-border bg-elevated px-2.5"
                      value={receivedAt}
                      onChange={(e) => setReceivedAt(e.target.value)}
                    />
                  </label>
                </>
              )}
              <div className="flex items-center gap-2">
                <Button variant="secondary" size="sm" onClick={pickSource}>
                  <FilePlus2 size={14} /> Asociar archivo…
                </Button>
                <span className="truncate text-[11px] text-muted">
                  {sourcePath ?? "Sin archivo (opcional)"}
                </span>
              </div>
              {error && (
                <div className="border border-error/40 bg-error/10 px-3 py-2 text-[12.5px] text-error">
                  {error}
                </div>
              )}
            </div>
            <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
              <Button
                variant="secondary"
                onClick={() => {
                  setCreateOpen(false);
                  setError(null);
                }}
              >
                Cancelar
              </Button>
              <Button
                disabled={!title.trim() || createMut.isPending}
                onClick={() => createMut.mutate()}
              >
                {createMut.isPending ? "Creando…" : "Crear"}
              </Button>
            </div>
          </div>
        </div>
      )}

      {derivedSource && categoryKey === "modelos" && (
        <CreateDerivedModelDialog
          sourceDocument={derivedSource}
          onClose={() => setDerivedSource(null)}
          onCompleted={async (message) => {
            setDerivedSource(null);
            await qc.invalidateQueries({
              queryKey: ["documents", projectId, categoryKey],
            });
            await qc.invalidateQueries({ queryKey: ["doc-versions"] });
            await qc.invalidateQueries({ queryKey: ["document"] });
            await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
            await qc.invalidateQueries({ queryKey: ["categories", projectId] });
            window.alert(message);
          }}
        />
      )}

      {unifyOpen && categoryKey === "modelos" && (
        <UnifyModelsDialog
          projectId={projectId}
          documents={docs.data ?? []}
          initialPrincipalId={selectedDocumentId}
          suggestedDuplicateIds={unifySuggestedDups}
          onClose={() => {
            setUnifyOpen(false);
            setUnifySuggestedDups([]);
          }}
          onCompleted={async (message) => {
            setUnifyOpen(false);
            setUnifySuggestedDups([]);
            await qc.invalidateQueries({
              queryKey: ["documents", projectId, categoryKey],
            });
            await qc.invalidateQueries({ queryKey: ["doc-versions"] });
            await qc.invalidateQueries({ queryKey: ["document"] });
            await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
            await qc.invalidateQueries({ queryKey: ["categories", projectId] });
            window.alert(message);
          }}
        />
      )}

      {repairHistoryOpen && categoryKey === "modelos" && (
        <RepairModelHistoryDialog
          projectId={projectId}
          documentId={selectedDocumentId}
          onClose={() => setRepairHistoryOpen(false)}
          onCompleted={async (message) => {
            setRepairHistoryOpen(false);
            await qc.invalidateQueries({
              queryKey: ["documents", projectId, categoryKey],
            });
            await qc.invalidateQueries({ queryKey: ["doc-versions"] });
            await qc.invalidateQueries({ queryKey: ["document"] });
            await qc.invalidateQueries({ queryKey: ["doc-version-files"] });
            await qc.invalidateQueries({ queryKey: ["categories", projectId] });
            window.alert(message);
          }}
        />
      )}
    </div>
  );
}

interface DocumentTreeRowsProps {
  doc: DocumentRow;
  projectId: string;
  categoryKey: string;
  usesRevisions: boolean;
  showCurrentVersionOnly: boolean;
  expanded: Set<string>;
  selectedDocumentId: string | null;
  onToggleExpand: (key: string, e?: ReactMouseEvent) => void;
  onSelectDoc: (id: string) => void;
  onOpenDoc: (id: string) => void;
  onOpenFile: (file: DocumentFileRow) => void;
  onDocContextMenu: (e: ReactMouseEvent, doc: DocumentRow) => void;
  onFileContextMenu: (
    e: ReactMouseEvent,
    doc: DocumentRow,
    file: DocumentFileRow,
  ) => void;
}

function DocumentTreeRows({
  doc,
  usesRevisions,
  showCurrentVersionOnly,
  expanded,
  selectedDocumentId,
  onToggleExpand,
  onSelectDoc,
  onOpenDoc,
  onOpenFile,
  onDocContextMenu,
  onFileContextMenu,
}: DocumentTreeRowsProps) {
  const docKey = treeNodeKey("doc", doc.id);
  const isOpen = expanded.has(docKey);

  const revisions = useQuery({
    queryKey: ["doc-revisions", doc.id],
    queryFn: () => api.listRevisions(doc.id),
    enabled: isOpen && usesRevisions,
  });

  const versions = useQuery({
    queryKey: ["doc-versions", doc.id],
    queryFn: () => api.listDocumentVersions(doc.id),
    enabled: isOpen && !usesRevisions,
  });

  const middleNodes: Array<
    | { kind: "rev"; row: RevisionRow }
    | { kind: "ver"; row: DocumentVersionRow }
  > = useMemo(() => {
    if (!isOpen) return [];
    if (usesRevisions) {
      return (revisions.data ?? []).map((row) => ({ kind: "rev" as const, row }));
    }
    let list = versions.data ?? [];
    if (showCurrentVersionOnly) {
      list = list.filter((v) => v.is_current === 1);
    }
    return list.map((row) => ({ kind: "ver" as const, row }));
  }, [
    isOpen,
    usesRevisions,
    revisions.data,
    versions.data,
    showCurrentVersionOnly,
  ]);

  return (
    <>
      <tr
        style={{ contentVisibility: "auto", containIntrinsicSize: "40px" }}
        className={cn(
          "h-[40px] cursor-pointer border-b border-border/50",
          selectedDocumentId === doc.id ? "bg-selection" : "hover:bg-elevated",
        )}
        onClick={() => onSelectDoc(doc.id)}
        onDoubleClick={() => onOpenDoc(doc.id)}
        onContextMenu={(e) => onDocContextMenu(e, doc)}
        title="Doble clic: abrir formato preferido de la revisión/versión actual · chevron: expandir"
      >
        <td className="px-3">
          <div className="flex min-w-0 items-center gap-1">
            <button
              type="button"
              className="inline-flex h-6 w-6 shrink-0 items-center justify-center text-muted hover:text-text"
              aria-label={isOpen ? "Colapsar" : "Expandir"}
              onClick={(e) => onToggleExpand(docKey, e)}
            >
              {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            </button>
            <span className="min-w-0 truncate">
              <span className="font-mono text-[12px] text-accent">
                {doc.business_code ?? "—"}
              </span>{" "}
              <span className="text-[13px]">{doc.title}</span>
              {doc.model_type && (
                <span className="ml-1 text-[10px] uppercase text-muted">
                  · {modelTypeLabel(doc.model_type)}
                </span>
              )}
              {doc.has_unregistered_change === 1 && (
                <span
                  title="Cambio detectado respecto a la versión registrada"
                  className="ml-1 inline-flex text-warn"
                >
                  <AlertTriangle size={12} />
                </span>
              )}
              {doc.file_count === 0 && (
                <span className="ml-1 text-[10px] uppercase text-error">
                  no disponible
                </span>
              )}
              {doc.subcategory && (
                <span className="ml-1 text-[11px] text-muted">
                  · {doc.subcategory}
                </span>
              )}
            </span>
          </div>
        </td>
        <td className="px-3 text-[12px] text-muted">{doc.status}</td>
        {usesRevisions && (
          <td className="px-3 font-mono text-[12px]">
            {doc.current_revision_code ? (
              <span className="inline-flex items-center gap-1">
                {doc.current_revision_code}
                <span className="text-[10px] uppercase text-muted">
                  {doc.current_revision_status === "issued" ? "emitida" : "borrador"}
                </span>
              </span>
            ) : (
              "—"
            )}
          </td>
        )}
        <td className="px-3 font-mono text-[12px]">
          {doc.current_version_label ?? "—"}
        </td>
        <td className="px-3 tabular-nums text-muted">{doc.file_count}</td>
        <td className="truncate px-3 text-[11px] text-muted">
          {doc.origin || doc.notes || "—"}
        </td>
      </tr>

      {isOpen && usesRevisions && revisions.isLoading && (
        <tr className="border-b border-border/40">
          <td colSpan={6} className="px-10 py-1 text-[11px] text-muted">
            Cargando revisiones…
          </td>
        </tr>
      )}
      {isOpen && !usesRevisions && versions.isLoading && (
        <tr className="border-b border-border/40">
          <td colSpan={5} className="px-10 py-1 text-[11px] text-muted">
            Cargando versiones…
          </td>
        </tr>
      )}

      {middleNodes.map((node) =>
        node.kind === "rev" ? (
          <RevisionBranch
            key={node.row.id}
            doc={doc}
            revision={node.row}
            usesRevisions={usesRevisions}
            expanded={expanded}
            onToggleExpand={onToggleExpand}
            onOpenFile={onOpenFile}
            onFileContextMenu={onFileContextMenu}
          />
        ) : (
          <VersionBranch
            key={node.row.id}
            doc={doc}
            version={node.row}
            usesRevisions={usesRevisions}
            expanded={expanded}
            onToggleExpand={onToggleExpand}
            onOpenFile={onOpenFile}
            onFileContextMenu={onFileContextMenu}
          />
        ),
      )}
    </>
  );
}

function RevisionBranch({
  doc,
  revision,
  usesRevisions,
  expanded,
  onToggleExpand,
  onOpenFile,
  onFileContextMenu,
}: {
  doc: DocumentRow;
  revision: RevisionRow;
  usesRevisions: boolean;
  expanded: Set<string>;
  onToggleExpand: (key: string, e?: ReactMouseEvent) => void;
  onOpenFile: (file: DocumentFileRow) => void;
  onFileContextMenu: (
    e: ReactMouseEvent,
    doc: DocumentRow,
    file: DocumentFileRow,
  ) => void;
}) {
  const key = treeNodeKey("rev", revision.id);
  const isOpen = expanded.has(key);
  const files = useQuery({
    queryKey: ["doc-revision-files", revision.id],
    queryFn: () => api.listRevisionFiles(revision.id),
    enabled: isOpen,
  });
  const colSpan = usesRevisions ? 6 : 5;

  return (
    <>
      <tr className="h-[34px] border-b border-border/40 hover:bg-elevated/60">
        <td className="px-3" colSpan={colSpan}>
          <div className="flex items-center gap-1 pl-6">
            <button
              type="button"
              className="inline-flex h-6 w-6 shrink-0 items-center justify-center text-muted hover:text-text"
              aria-label={isOpen ? "Colapsar revisión" : "Expandir revisión"}
              onClick={(e) => onToggleExpand(key, e)}
            >
              {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            </button>
            <span className="font-mono text-[12px] text-accent">
              Rev. {revision.revision_code}
            </span>
            <span className="text-[11px] text-muted">
              {revision.status === "issued"
                ? "emitida"
                : revision.status === "draft"
                  ? "borrador"
                  : (revision.status ?? "")}
            </span>
            {files.data && (
              <span className="text-[11px] text-muted">
                · {files.data.length} archivo(s)
              </span>
            )}
          </div>
        </td>
      </tr>
      {isOpen && files.isLoading && (
        <tr className="border-b border-border/30">
          <td colSpan={colSpan} className="px-16 py-1 text-[11px] text-muted">
            Cargando archivos…
          </td>
        </tr>
      )}
      {isOpen &&
        files.data?.map((f) => (
          <FileLeafRow
            key={f.id}
            doc={doc}
            file={f}
            depth={2}
            colSpan={colSpan}
            onOpenFile={onOpenFile}
            onFileContextMenu={onFileContextMenu}
          />
        ))}
    </>
  );
}

function VersionBranch({
  doc,
  version,
  usesRevisions,
  expanded,
  onToggleExpand,
  onOpenFile,
  onFileContextMenu,
}: {
  doc: DocumentRow;
  version: DocumentVersionRow;
  usesRevisions: boolean;
  expanded: Set<string>;
  onToggleExpand: (key: string, e?: ReactMouseEvent) => void;
  onOpenFile: (file: DocumentFileRow) => void;
  onFileContextMenu: (
    e: ReactMouseEvent,
    doc: DocumentRow,
    file: DocumentFileRow,
  ) => void;
}) {
  const key = treeNodeKey("ver", version.id);
  const isOpen = expanded.has(key);
  const files = useQuery({
    queryKey: ["doc-version-files", version.id],
    queryFn: () => api.listVersionFiles(version.id),
    enabled: isOpen,
  });
  const colSpan = usesRevisions ? 6 : 5;

  return (
    <>
      <tr className="h-[34px] border-b border-border/40 hover:bg-elevated/60">
        <td className="px-3" colSpan={colSpan}>
          <div className="flex items-center gap-1 pl-6">
            <button
              type="button"
              className="inline-flex h-6 w-6 shrink-0 items-center justify-center text-muted hover:text-text"
              aria-label={isOpen ? "Colapsar versión" : "Expandir versión"}
              onClick={(e) => onToggleExpand(key, e)}
            >
              {isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            </button>
            <span className="font-mono text-[12px]">
              {version.label ?? `v${version.version_number}`}
            </span>
            {version.is_current === 1 && (
              <span className="text-[10px] uppercase text-muted">actual</span>
            )}
            {files.data && (
              <span className="text-[11px] text-muted">
                · {files.data.length} archivo(s)
              </span>
            )}
          </div>
        </td>
      </tr>
      {isOpen && files.isLoading && (
        <tr className="border-b border-border/30">
          <td colSpan={colSpan} className="px-16 py-1 text-[11px] text-muted">
            Cargando archivos…
          </td>
        </tr>
      )}
      {isOpen &&
        files.data?.map((f) => (
          <FileLeafRow
            key={f.id}
            doc={doc}
            file={f}
            depth={2}
            colSpan={colSpan}
            onOpenFile={onOpenFile}
            onFileContextMenu={onFileContextMenu}
          />
        ))}
    </>
  );
}

function FileLeafRow({
  doc,
  file,
  depth,
  colSpan,
  onOpenFile,
  onFileContextMenu,
}: {
  doc: DocumentRow;
  file: DocumentFileRow;
  depth: number;
  colSpan: number;
  onOpenFile: (file: DocumentFileRow) => void;
  onFileContextMenu: (
    e: ReactMouseEvent,
    doc: DocumentRow,
    file: DocumentFileRow,
  ) => void;
}) {
  return (
    <tr
      className="h-[32px] cursor-pointer border-b border-border/30 hover:bg-elevated/50"
      onDoubleClick={() => onOpenFile(file)}
      onContextMenu={(e) => onFileContextMenu(e, doc, file)}
      title="Doble clic: abrir este archivo"
    >
      <td className="px-3" colSpan={colSpan}>
        <div
          className="flex min-w-0 items-center gap-2 text-[12px]"
          style={{ paddingLeft: 12 + depth * 16 }}
        >
          <span className="truncate font-mono text-[11.5px]">{file.file_name}</span>
          {file.sheet_kind === "calc" && (
            <span className="text-[10px] text-muted">
              [CALC
              {file.relative_path.includes(".ingmanager/snapshots/")
                ? " · histórico"
                : ""}
              ]
            </span>
          )}
          {file.sheet_kind === "export" && (
            <span className="text-[10px] text-muted">[EXPORT]</span>
          )}
          {!file.sheet_kind && file.format_label && (
            <span className="text-[10px] text-muted">[{file.format_label}]</span>
          )}
          {file.is_available !== 1 && (
            <span className="text-[10px] text-error">no disponible</span>
          )}
        </div>
      </td>
    </tr>
  );
}
