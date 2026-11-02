import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, FilePlus2, Plus } from "lucide-react";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";

interface Props {
  projectId: string;
  categoryKey: string;
}

const MEMORIA_STATUSES = ["desarrollo", "revision", "aprobada", "obsoleta"];

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
  const [error, setError] = useState<string | null>(null);

  const docs = useQuery({
    queryKey: ["documents", projectId, categoryKey],
    queryFn: () => api.listDocuments(projectId, categoryKey, true),
  });

  const codePreview = useQuery({
    queryKey: ["doc-code", projectId, categoryKey, assignCode],
    queryFn: () => api.previewNextDocumentCode(projectId, categoryKey),
    enabled: createOpen && assignCode,
  });

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
      }),
    onSuccess: async (doc) => {
      await qc.invalidateQueries({ queryKey: ["documents", projectId, categoryKey] });
      setCreateOpen(false);
      setTitle("");
      setSourcePath(null);
      setSelectedDocumentId(doc.id);
      setInspectorTab("doc-detail");
    },
    onError: (e: Error) => setError(String(e)),
  });

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

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-border bg-panel px-3 py-1.5">
        <Button size="sm" onClick={() => setCreateOpen(true)}>
          <Plus size={14} /> Nuevo documento
        </Button>
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

      <div className="min-h-0 flex-1 overflow-auto">
        {docs.isLoading && <div className="p-4 text-muted">Cargando documentos…</div>}
        {docs.error && (
          <div className="p-4 text-error">{String(docs.error)}</div>
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
        {(docs.data?.length ?? 0) > 0 && (
          <table className="w-full border-collapse text-left">
            <thead className="sticky top-0 bg-panel">
              <tr className="border-b border-border text-[11px] uppercase text-muted">
                <th className="h-[36px] px-3 font-medium">Código</th>
                <th className="px-3 font-medium">Título</th>
                <th className="w-[100px] px-3 font-medium">Estado</th>
                <th className="w-[70px] px-3 font-medium">Versión</th>
                <th className="w-[70px] px-3 font-medium">Archivos</th>
                <th className="w-[110px] px-3 font-medium">Notas</th>
              </tr>
            </thead>
            <tbody>
              {docs.data?.map((d) => (
                <tr
                  key={d.id}
                  className={cn(
                    "h-[40px] cursor-pointer border-b border-border/50",
                    selectedDocumentId === d.id
                      ? "bg-selection"
                      : "hover:bg-elevated",
                  )}
                  onClick={() => {
                    setSelectedDocumentId(d.id);
                    setInspectorTab("doc-detail");
                  }}
                >
                  <td className="px-3 font-mono text-[12px] text-accent">
                    {d.business_code ?? "—"}
                  </td>
                  <td className="truncate px-3">
                    <span className="inline-flex items-center gap-1.5">
                      {d.title}
                      {d.has_unregistered_change === 1 && (
                        <span
                          title="Cambio detectado respecto a la versión registrada"
                          className="inline-flex text-warn"
                        >
                          <AlertTriangle size={12} />
                        </span>
                      )}
                    </span>
                    {d.subcategory && (
                      <div className="text-[11px] text-muted">{d.subcategory}</div>
                    )}
                  </td>
                  <td className="px-3 text-[12px] text-muted">{d.status}</td>
                  <td className="px-3 font-mono text-[12px]">
                    {showCurrentVersionOnly
                      ? (d.current_version_label ?? "—")
                      : (d.current_version_label ?? "—")}
                  </td>
                  <td className="px-3 tabular-nums text-muted">{d.file_count}</td>
                  <td className="truncate px-3 text-[11px] text-muted">
                    {d.origin || d.notes || "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

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
    </div>
  );
}
