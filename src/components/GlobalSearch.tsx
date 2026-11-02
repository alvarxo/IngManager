import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { FileText, FolderKanban, Search, X } from "lucide-react";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";

interface Props {
  open: boolean;
  onClose: () => void;
}

export function GlobalSearch({ open, onClose }: Props) {
  const setView = useUiStore((s) => s.setView);
  const setSelectedDocumentId = useUiStore((s) => s.setSelectedDocumentId);
  const setInspectorTab = useUiStore((s) => s.setInspectorTab);
  const setProjectMainTab = useUiStore((s) => s.setProjectMainTab);
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);

  const deferred = useDeferredQuery(query, 180);

  const results = useQuery({
    queryKey: ["global-search", deferred],
    queryFn: () => api.globalSearch(deferred, 40),
    enabled: open && deferred.trim().length >= 1,
  });

  const hits = results.data ?? [];

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
    const t = window.setTimeout(() => inputRef.current?.focus(), 20);
    return () => window.clearTimeout(t);
  }, [open]);

  useEffect(() => {
    setActive(0);
  }, [deferred]);

  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setActive((i) => Math.min(i + 1, Math.max(hits.length - 1, 0)));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActive((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter" && hits[active]) {
        e.preventDefault();
        void openHit(hits[active]!);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, hits, active, onClose]);

  async function openHit(hit: (typeof hits)[number]) {
    onClose();
    if (hit.entity_type === "project") {
      await api.openProject(hit.entity_id);
      setSelectedDocumentId(null);
      setProjectMainTab("documents");
      setView({ kind: "project", projectId: hit.entity_id });
      return;
    }
    if (hit.entity_type === "transmission" && hit.project_id) {
      await api.openProject(hit.project_id);
      setSelectedDocumentId(null);
      setProjectMainTab("transmissions");
      setView({ kind: "project", projectId: hit.project_id });
      return;
    }
    if (hit.project_id) {
      await api.openProject(hit.project_id);
      setProjectMainTab("documents");
      setView({ kind: "project", projectId: hit.project_id });
    }
    if (hit.entity_type === "document") {
      setSelectedDocumentId(hit.entity_id);
      setInspectorTab("doc-detail");
    } else {
      setSelectedDocumentId(null);
    }
  }

  const emptyHint = useMemo(() => {
    if (!deferred.trim()) return "Escriba un código, título o nombre de archivo.";
    if (results.isFetching) return "Buscando…";
    if (results.isError) return String(results.error);
    return "Sin resultados.";
  }, [deferred, results.isFetching, results.isError, results.error]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/45 pt-[12vh]"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="w-[min(640px,92vw)] overflow-hidden rounded-[8px] border border-border bg-panel shadow-xl">
        <div className="flex h-[48px] items-center gap-2 border-b border-border px-3">
          <Search size={16} className="text-muted" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Buscar proyectos, documentos, archivos…"
            className="h-full flex-1 bg-transparent text-[14px] outline-none placeholder:text-muted"
          />
          <kbd className="rounded border border-border px-1.5 text-[10px] text-muted">
            Esc
          </kbd>
          <button
            type="button"
            className="rounded p-1 text-muted hover:bg-elevated hover:text-text"
            onClick={onClose}
            aria-label="Cerrar"
          >
            <X size={16} />
          </button>
        </div>

        <div className="max-h-[50vh] overflow-auto">
          {hits.length === 0 ? (
            <p className="px-4 py-6 text-[13px] text-muted">{emptyHint}</p>
          ) : (
            <ul>
              {hits.map((hit, i) => {
                const isProject = hit.entity_type === "project";
                return (
                  <li key={`${hit.entity_type}-${hit.entity_id}-${i}`}>
                    <button
                      type="button"
                      onClick={() => void openHit(hit)}
                      onMouseEnter={() => setActive(i)}
                      className={cn(
                        "flex w-full items-start gap-3 px-4 py-2.5 text-left",
                        i === active ? "bg-selection" : "hover:bg-elevated",
                      )}
                    >
                      <span className="mt-0.5 text-muted">
                        {isProject ? (
                          <FolderKanban size={15} />
                        ) : (
                          <FileText size={15} />
                        )}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-2">
                          {hit.code && (
                            <span className="font-mono text-[12px] text-accent">
                              {hit.code}
                            </span>
                          )}
                          <span className="truncate text-[13px] font-medium">
                            {hit.title || hit.file_name || hit.entity_type}
                          </span>
                        </span>
                        <span className="block truncate text-[11.5px] text-muted">
                          {hit.entity_type}
                          {hit.file_name ? ` · ${hit.file_name}` : ""}
                          {hit.body ? ` · ${hit.body.slice(0, 80)}` : ""}
                        </span>
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}

function useDeferredQuery(value: string, delayMs: number) {
  const [deferred, setDeferred] = useState(value);
  useEffect(() => {
    const t = window.setTimeout(() => setDeferred(value), delayMs);
    return () => window.clearTimeout(t);
  }, [value, delayMs]);
  return deferred;
}
