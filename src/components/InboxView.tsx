import { useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Inbox, Trash2, FolderInput } from "lucide-react";
import { api } from "@/lib/api";
import { useNativeFileDrop } from "@/hooks/useNativeFileDrop";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const CATEGORIES = [
  { key: "antecedentes", label: "Antecedentes" },
  { key: "modelos", label: "Modelos" },
  { key: "planos", label: "Planos" },
  { key: "memorias", label: "Memorias" },
  { key: "admin", label: "Administración" },
  { key: "otros", label: "Otros" },
];

export function InboxView() {
  const qc = useQueryClient();
  const [selected, setSelected] = useState<string[]>([]);
  const [projectIds, setProjectIds] = useState<string[]>([]);
  const [categoryKey, setCategoryKey] = useState("modelos");
  const [mode, setMode] = useState<"copy" | "move">("copy");
  const [message, setMessage] = useState<string | null>(null);
  const [osDragOver, setOsDragOver] = useState(false);
  const dropZoneRef = useRef<HTMLDivElement>(null);

  const items = useQuery({
    queryKey: ["inbox"],
    queryFn: () => api.inboxList(),
  });
  const projects = useQuery({
    queryKey: ["projects"],
    queryFn: () => api.listProjects(false),
  });

  const stage = useMutation({
    mutationFn: (paths: string[]) => api.inboxStageFiles(paths, true),
    onSuccess: (staged) => {
      void qc.invalidateQueries({ queryKey: ["inbox"] });
      setMessage(
        staged.length
          ? `${staged.length} archivo(s) añadidos a la bandeja`
          : "Archivos añadidos a la bandeja",
      );
    },
    onError: (e) => setMessage(String(e)),
  });

  const assign = useMutation({
    mutationFn: () =>
      api.inboxAssign({
        item_ids: selected,
        project_ids: projectIds,
        category_key: categoryKey,
        mode,
      }),
    onSuccess: (r) => {
      void qc.invalidateQueries({ queryKey: ["inbox"] });
      setMessage(
        `Asignados: ${r.assigned}${r.failures.length ? ` · fallos: ${r.failures.length}` : ""}`,
      );
      setSelected([]);
    },
    onError: (e) => setMessage(String(e)),
  });

  const remove = useMutation({
    mutationFn: (ids: string[]) => api.inboxRemoveItems(ids, true),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["inbox"] });
      setSelected([]);
    },
  });

  const pickFiles = async () => {
    const picked = await open({
      multiple: true,
      directory: false,
      title: "Añadir a bandeja global",
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    stage.mutate(paths);
  };

  useNativeFileDrop(dropZoneRef, {
    onDrop: (paths) => {
      if (paths.length === 0) {
        setMessage(
          "No se recibieron rutas del SO. Use «Añadir archivos» (diálogo).",
        );
        return;
      }
      stage.mutate(paths);
    },
    onActiveChange: setOsDragOver,
  });

  const toggleProject = (id: string) => {
    setProjectIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );
  };

  const rows = useMemo(() => items.data ?? [], [items.data]);

  return (
    <div className="flex h-full flex-col bg-bg">
      <header className="flex items-center justify-between border-b border-border px-4 py-3">
        <div>
          <h1 className="text-[15px] font-semibold text-text">Bandeja global</h1>
          <p className="text-[12px] text-muted">
            Staging · asignación multi-proyecto · copia por defecto · drop nativo
            Tauri (rutas reales)
          </p>
        </div>
        <div className="flex gap-2">
          <Button type="button" variant="secondary" onClick={() => void pickFiles()}>
            <FolderInput size={14} className="mr-1.5" />
            Añadir archivos
          </Button>
          <Button
            type="button"
            variant="ghost"
            disabled={selected.length === 0}
            onClick={() => remove.mutate(selected)}
          >
            <Trash2 size={14} className="mr-1.5" />
            Quitar
          </Button>
        </div>
      </header>

      <div
        ref={dropZoneRef}
        className={cn(
          "mx-4 mt-3 flex min-h-[88px] items-center justify-center rounded-[var(--radius-sm)] border border-dashed px-3 text-[12px]",
          osDragOver
            ? "border-accent bg-selection text-text"
            : "border-border text-muted",
        )}
      >
        <Inbox size={16} className="mr-2 shrink-0" />
        Arrastre archivos o carpetas desde el Explorador (rutas nativas Tauri) o
        use «Añadir archivos»
      </div>

      {message && (
        <div className="mx-4 mt-2 text-[12px] text-muted">{message}</div>
      )}

      <div className="grid min-h-0 flex-1 grid-cols-[1fr_280px] gap-0">
        <div className="min-h-0 overflow-auto border-r border-border p-4">
          <table className="w-full text-left text-[13px]">
            <thead className="sticky top-0 bg-bg text-[11px] uppercase tracking-wide text-muted">
              <tr>
                <th className="w-8 py-2" />
                <th className="py-2 font-medium">Archivo</th>
                <th className="py-2 font-medium">Tamaño</th>
                <th className="py-2 font-medium">Entrada</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((item) => (
                <tr
                  key={item.id}
                  className="border-t border-border/60 hover:bg-elevated"
                >
                  <td className="py-1.5">
                    <input
                      type="checkbox"
                      checked={selected.includes(item.id)}
                      onChange={() =>
                        setSelected((prev) =>
                          prev.includes(item.id)
                            ? prev.filter((x) => x !== item.id)
                            : [...prev, item.id],
                        )
                      }
                    />
                  </td>
                  <td className="py-1.5 text-text">{item.file_name}</td>
                  <td className="py-1.5 text-muted">
                    {item.size_bytes != null
                      ? `${Math.round(item.size_bytes / 1024)} KB`
                      : "—"}
                  </td>
                  <td className="py-1.5 text-muted">{item.staged_at}</td>
                </tr>
              ))}
              {rows.length === 0 && (
                <tr>
                  <td colSpan={4} className="py-8 text-center text-muted">
                    Bandeja vacía
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        <aside className="flex flex-col gap-3 overflow-auto p-4">
          <div>
            <div className="mb-1 text-[11px] font-semibold uppercase text-muted">
              Categoría destino
            </div>
            <select
              className="h-[34px] w-full rounded-[var(--radius-sm)] border border-border bg-elevated px-2 text-[13px] text-text"
              value={categoryKey}
              onChange={(e) => setCategoryKey(e.target.value)}
            >
              {CATEGORIES.map((c) => (
                <option key={c.key} value={c.key}>
                  {c.label}
                </option>
              ))}
            </select>
          </div>
          <div>
            <div className="mb-1 text-[11px] font-semibold uppercase text-muted">
              Modo
            </div>
            <select
              className="h-[34px] w-full rounded-[var(--radius-sm)] border border-border bg-elevated px-2 text-[13px] text-text"
              value={mode}
              onChange={(e) => setMode(e.target.value as "copy" | "move")}
            >
              <option value="copy">Copiar (default)</option>
              <option value="move">Mover</option>
            </select>
          </div>
          <div>
            <div className="mb-1 text-[11px] font-semibold uppercase text-muted">
              Proyectos ({projectIds.length})
            </div>
            <ul className="max-h-[280px] space-y-1 overflow-auto">
              {(projects.data ?? []).map((p) => (
                <li key={p.id}>
                  <label className="flex cursor-pointer items-center gap-2 text-[13px] text-text">
                    <input
                      type="checkbox"
                      checked={projectIds.includes(p.id)}
                      onChange={() => toggleProject(p.id)}
                    />
                    <span className="truncate">
                      {p.code ? `${p.code} · ` : ""}
                      {p.name}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          </div>
          <Button
            type="button"
            disabled={
              selected.length === 0 ||
              projectIds.length === 0 ||
              assign.isPending
            }
            onClick={() => assign.mutate()}
          >
            Asignar selección
          </Button>
        </aside>
      </div>
    </div>
  );
}
