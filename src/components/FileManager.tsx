import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ArrowRight,
  Copy,
  ClipboardPaste,
  FilePlus2,
  FolderOpen,
  FolderPlus,
  Pencil,
  RefreshCw,
  Scissors,
  Search,
  Trash2,
  AlertTriangle,
} from "lucide-react";
import { api } from "@/lib/api";
import type { FsEntry } from "@/lib/types";
import { cn, formatBytes } from "@/lib/utils";
import {
  filterAndSortEntries,
  filterEngineeringAuxiliaries,
  parentSubpath,
  type SortDir,
  type SortKey,
} from "@/lib/file-utils";
import { useNativeFileDrop } from "@/hooks/useNativeFileDrop";
import { useClipboardStore } from "@/stores/clipboard-store";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";
import { IncorporationDialog } from "./IncorporationDialog";

interface Props {
  projectId: string;
  categoryKey: string;
}

interface ContextMenuState {
  x: number;
  y: number;
  entry: FsEntry | null;
}

export function FileManager({ projectId, categoryKey }: Props) {
  const qc = useQueryClient();
  const setSelectedPaths = useUiStore((s) => s.setSelectedPaths);
  const setInspectorTab = useUiStore((s) => s.setInspectorTab);
  const clipboard = useClipboardStore();

  const [subpath, setSubpath] = useState("");
  const [history, setHistory] = useState<string[]>([""]);
  const [histIdx, setHistIdx] = useState(0);
  const [selected, setSelected] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [status, setStatus] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [pendingAck, setPendingAck] = useState<null | (() => Promise<void>)>(
    null,
  );
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [osDragOver, setOsDragOver] = useState(false);
  const [showAuxiliaries, setShowAuxiliaries] = useState(false);
  const [incorporate, setIncorporate] = useState<{
    paths: string[];
    alreadyInProject: boolean;
    preferMove?: boolean;
  } | null>(null);
  const tableRef = useRef<HTMLDivElement>(null);
  const dropZoneRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setSubpath("");
    setHistory([""]);
    setHistIdx(0);
    setSelected([]);
    setFilter("");
  }, [projectId, categoryKey]);

  const folderQuery = useQuery({
    queryKey: ["fm-folder", projectId, categoryKey, subpath],
    queryFn: () =>
      api.resolveProjectCategoryPath(projectId, categoryKey, subpath || null),
  });

  const currentPath = folderQuery.data?.absolute_path;

  const entriesQuery = useQuery({
    queryKey: ["fm-entries", currentPath],
    queryFn: () => api.listDirectory(currentPath!),
    enabled: !!currentPath,
  });

  const entries = useMemo(() => {
    const raw = entriesQuery.data ?? [];
    const filtered =
      categoryKey === "modelos"
        ? filterEngineeringAuxiliaries(raw, showAuxiliaries)
        : raw;
    return filterAndSortEntries(filtered, filter, sortKey, sortDir);
  }, [
    entriesQuery.data,
    filter,
    sortKey,
    sortDir,
    categoryKey,
    showAuxiliaries,
  ]);

  const refresh = useCallback(async () => {
    await qc.invalidateQueries({ queryKey: ["fm-folder", projectId, categoryKey] });
    await qc.invalidateQueries({ queryKey: ["fm-entries"] });
    await qc.invalidateQueries({ queryKey: ["categories", projectId] });
    await qc.invalidateQueries({ queryKey: ["trash"] });
  }, [qc, projectId, categoryKey]);

  useEffect(() => {
    setSelectedPaths(selected);
  }, [selected, setSelectedPaths]);

  function navigateTo(next: string, pushHistory = true) {
    setSubpath(next);
    setSelected([]);
    if (pushHistory) {
      const sliced = history.slice(0, histIdx + 1);
      sliced.push(next);
      setHistory(sliced);
      setHistIdx(sliced.length - 1);
    }
  }

  function goBack() {
    if (histIdx <= 0) return;
    const idx = histIdx - 1;
    setHistIdx(idx);
    setSubpath(history[idx] ?? "");
    setSelected([]);
  }

  function goForward() {
    if (histIdx >= history.length - 1) return;
    const idx = histIdx + 1;
    setHistIdx(idx);
    setSubpath(history[idx] ?? "");
    setSelected([]);
  }

  function goUp() {
    navigateTo(parentSubpath(subpath));
  }

  async function runOp(
    op: () => Promise<{ warning: string | null; message: string | null }>,
  ) {
    try {
      setStatus(null);
      const result = await op();
      if (result.warning && result.message?.includes("confirmación")) {
        setWarning(result.warning);
        return "needs_ack" as const;
      }
      if (result.warning) setWarning(result.warning);
      else setWarning(null);
      setStatus(result.message);
      await refresh();
      return "ok" as const;
    } catch (e) {
      setStatus(String(e));
      return "error" as const;
    }
  }

  async function withEngineeringAck(
    first: () => Promise<{ warning: string | null; message: string | null }>,
    retry: () => Promise<{ warning: string | null; message: string | null }>,
  ) {
    const outcome = await runOp(first);
    if (outcome === "needs_ack") {
      setPendingAck(() => async () => {
        setWarning(null);
        setPendingAck(null);
        await runOp(retry);
      });
    }
  }

  async function handleOpen(entry: FsEntry) {
    if (entry.is_directory) {
      const base = folderQuery.data?.relative_path ?? "";
      const projectPrefix = base.split("/").slice(0, 1).join("/");
      // relative_path is root-relative: CODE/01_.../sub
      const catRoot = base;
      let next = "";
      if (entry.relative_path.startsWith(`${catRoot}/`)) {
        next = entry.relative_path.slice(catRoot.length + 1);
      } else if (entry.relative_path.startsWith(catRoot)) {
        next = entry.relative_path.slice(catRoot.length).replace(/^\//, "");
      }
      void projectPrefix;
      navigateTo(next);
    } else {
      await api.openFile(entry.absolute_path);
    }
  }

  async function createFolder() {
    if (!currentPath) return;
    const name = window.prompt("Nombre de la carpeta");
    if (!name) return;
    await runOp(() => api.createFolder(currentPath, name));
  }

  async function renameSelected() {
    const path = selected[0];
    if (!path) return;
    const entry = entries.find((e) => e.absolute_path === path);
    const name = window.prompt("Nuevo nombre", entry?.name ?? "");
    if (!name || name === entry?.name) return;
    await withEngineeringAck(
      () => api.renameEntry(path, name, false),
      () => api.renameEntry(path, name, true),
    );
  }

  async function importFiles() {
    if (!currentPath) return;
    const picked = await open({
      multiple: true,
      directory: false,
      title: "Importar archivos (copia)",
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    await runOp(() => api.importFiles(paths, currentPath));
  }

  async function copySelected() {
    if (!selected.length) return;
    clipboard.setClipboard("copy", selected);
    setStatus(`${selected.length} elemento(s) en portapapeles (copiar)`);
  }

  async function cutSelected() {
    if (!selected.length) return;
    clipboard.setClipboard("cut", selected);
    setStatus(`${selected.length} elemento(s) en portapapeles (cortar)`);
  }

  async function pasteHere() {
    if (!currentPath || !clipboard.mode || clipboard.paths.length === 0) return;
    if (clipboard.mode === "copy") {
      await runOp(() => api.copyEntries(clipboard.paths, currentPath));
    } else {
      await withEngineeringAck(
        () => api.moveEntries(clipboard.paths, currentPath, "rename", false),
        () => api.moveEntries(clipboard.paths, currentPath, "rename", true),
      );
      clipboard.clear();
    }
  }

  async function deleteSelected() {
    if (!selected.length) return;
    if (!window.confirm(`¿Enviar ${selected.length} elemento(s) a la papelera?`)) {
      return;
    }
    await withEngineeringAck(
      () => api.deleteToTrash(selected, false),
      () => api.deleteToTrash(selected, true),
    );
    setSelected([]);
  }

  function onRowClick(e: ReactMouseEvent, entry: FsEntry) {
    if (e.ctrlKey || e.metaKey) {
      setSelected((prev) =>
        prev.includes(entry.absolute_path)
          ? prev.filter((p) => p !== entry.absolute_path)
          : [...prev, entry.absolute_path],
      );
    } else if (e.shiftKey && selected.length) {
      const ids = entries.map((x) => x.absolute_path);
      const last = ids.indexOf(selected[selected.length - 1]!);
      const cur = ids.indexOf(entry.absolute_path);
      if (last >= 0 && cur >= 0) {
        const [a, b] = last < cur ? [last, cur] : [cur, last];
        setSelected(ids.slice(a, b + 1));
      }
    } else {
      setSelected([entry.absolute_path]);
      setInspectorTab("file");
    }
  }

  function onContextMenu(e: ReactMouseEvent, entry: FsEntry | null) {
    e.preventDefault();
    if (entry && !selected.includes(entry.absolute_path)) {
      setSelected([entry.absolute_path]);
    }
    setContextMenu({ x: e.clientX, y: e.clientY, entry });
  }

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      ) {
        return;
      }
      if (!tableRef.current) return;

      if (e.key === "F2") {
        e.preventDefault();
        void renameSelected();
      } else if (e.key === "Delete") {
        e.preventDefault();
        void deleteSelected();
      } else if (e.key === "Enter" && selected.length === 1) {
        const entry = entries.find((x) => x.absolute_path === selected[0]);
        if (entry) void handleOpen(entry);
      } else if (e.altKey && e.key === "ArrowLeft") {
        e.preventDefault();
        goBack();
      } else if (e.altKey && e.key === "ArrowRight") {
        e.preventDefault();
        goForward();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c") {
        e.preventDefault();
        void copySelected();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "x") {
        e.preventDefault();
        void cutSelected();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "v") {
        e.preventDefault();
        void pasteHere();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected, entries, currentPath, clipboard, histIdx, history, subpath]);

  function onDragStart(e: DragEvent, entry: FsEntry) {
    const paths = selected.includes(entry.absolute_path)
      ? selected
      : [entry.absolute_path];
    e.dataTransfer.setData(
      "application/x-ingmanager-paths",
      JSON.stringify(paths),
    );
    e.dataTransfer.effectAllowed = "copyMove";
  }

  /** Internal moves/copies only — OS Explorer drops use Tauri onDragDropEvent. */
  async function onInternalDrop(e: DragEvent) {
    e.preventDefault();
    setDragOver(false);
    if (!currentPath) return;
    const raw = e.dataTransfer.getData("application/x-ingmanager-paths");
    if (!raw) return;
    const paths = JSON.parse(raw) as string[];
    const move = !e.ctrlKey;
    if (move) {
      await withEngineeringAck(
        () => api.moveEntries(paths, currentPath, "rename", false),
        () => api.moveEntries(paths, currentPath, "rename", true),
      );
    } else {
      await runOp(() => api.copyEntries(paths, currentPath));
    }
  }

  useNativeFileDrop(dropZoneRef, {
    enabled: !!currentPath && !incorporate,
    onDrop: () => undefined,
    onDropWithModifiers: (paths, preferMove) => {
      if (!currentPath || paths.length === 0) return;
      setIncorporate({
        paths,
        alreadyInProject: false,
        preferMove,
      });
    },
    onActiveChange: setOsDragOver,
  });

  function toggleSort(key: SortKey) {
    if (sortKey === key) setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    else {
      setSortKey(key);
      setSortDir("asc");
    }
  }

  const breadcrumb = [categoryKey, ...subpath.split("/").filter(Boolean)].join(
    " / ",
  );

  return (
    <div
      className="flex h-full min-w-0 flex-1 flex-col"
      ref={tableRef}
      tabIndex={0}
      onClick={() => setContextMenu(null)}
      onKeyDown={(_e: ReactKeyboardEvent) => undefined}
    >
      <div className="flex flex-wrap items-center gap-1 border-b border-border bg-panel px-2 py-1.5">
        <Button variant="ghost" size="icon" onClick={goBack} title="Atrás (Alt+←)">
          <ArrowLeft size={14} />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={goForward}
          title="Adelante (Alt+→)"
        >
          <ArrowRight size={14} />
        </Button>
        <Button variant="ghost" size="sm" onClick={goUp} disabled={!subpath}>
          Subir
        </Button>
        <div className="mx-2 min-w-0 flex-1 truncate text-[12px] text-muted">
          {breadcrumb}
        </div>
        <div className="flex h-[28px] w-[180px] items-center gap-1 rounded-[4px] border border-border bg-elevated px-2">
          <Search size={13} className="text-muted" />
          <input
            className="w-full bg-transparent text-[12px] outline-none"
            placeholder="Filtrar…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        {categoryKey === "modelos" && (
          <label className="ml-1 flex items-center gap-1.5 text-[11.5px] text-muted">
            <input
              type="checkbox"
              checked={showAuxiliaries}
              onChange={(e) => setShowAuxiliaries(e.target.checked)}
            />
            Mostrar auxiliares
          </label>
        )}
        <Button variant="secondary" size="sm" onClick={createFolder}>
          <FolderPlus size={14} /> Carpeta
        </Button>
        <Button variant="secondary" size="sm" onClick={importFiles}>
          <FilePlus2 size={14} /> Importar
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={
            selected.filter((p) =>
              entries.some((e) => e.absolute_path === p && !e.is_directory),
            ).length === 0
          }
          onClick={() => {
            const paths = selected.filter((p) =>
              entries.some((e) => e.absolute_path === p && !e.is_directory),
            );
            if (paths.length) {
              setIncorporate({ paths, alreadyInProject: true });
            }
          }}
          title="Registrar o asociar archivos ya en el proyecto (sin volver a arrastrar)"
        >
          Incorporar
        </Button>
        <Button variant="ghost" size="icon" onClick={copySelected} title="Copiar">
          <Copy size={14} />
        </Button>
        <Button variant="ghost" size="icon" onClick={cutSelected} title="Cortar">
          <Scissors size={14} />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={pasteHere}
          title="Pegar"
          disabled={!clipboard.mode}
        >
          <ClipboardPaste size={14} />
        </Button>
        <Button variant="ghost" size="icon" onClick={renameSelected} title="Renombrar (F2)">
          <Pencil size={14} />
        </Button>
        <Button variant="ghost" size="icon" onClick={deleteSelected} title="Eliminar">
          <Trash2 size={14} />
        </Button>
        <Button variant="ghost" size="icon" onClick={() => refresh()} title="Actualizar">
          <RefreshCw size={14} />
        </Button>
      </div>

      {(status || warning) && (
        <div className="flex items-start gap-2 border-b border-border bg-elevated px-3 py-2 text-[12px]">
          {warning && <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warn" />}
          <div className="min-w-0 flex-1">
            {warning && <div className="text-warn">{warning}</div>}
            {status && <div className="text-muted">{status}</div>}
          </div>
          {pendingAck && (
            <Button size="sm" onClick={() => pendingAck()}>
              Confirmar de todos modos
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setWarning(null);
              setStatus(null);
              setPendingAck(null);
            }}
          >
            Cerrar
          </Button>
        </div>
      )}

      <div
        ref={dropZoneRef}
        className={cn(
          "min-h-0 flex-1 overflow-auto",
          (dragOver || osDragOver) && "outline outline-1 outline-accent",
        )}
        onDragOver={(e) => {
          // Allow HTML5 internal row drags; OS file drops are handled natively.
          if (e.dataTransfer.types.includes("application/x-ingmanager-paths")) {
            e.preventDefault();
            setDragOver(true);
          }
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={onInternalDrop}
        onContextMenu={(e) => onContextMenu(e, null)}
      >
        {folderQuery.isLoading || entriesQuery.isLoading ? (
          <div className="p-4 text-muted">Cargando…</div>
        ) : entriesQuery.error ? (
          <div className="p-4 text-error">{String(entriesQuery.error)}</div>
        ) : entries.length === 0 ? (
          <div className="p-8 text-center text-muted">
            Carpeta vacía. Cree una carpeta o importe archivos.
          </div>
        ) : (
          <table className="w-full border-collapse text-left">
            <thead className="sticky top-0 bg-panel">
              <tr className="border-b border-border text-[11px] uppercase text-muted">
                <th
                  className="h-[36px] cursor-pointer px-3 font-medium"
                  onClick={() => toggleSort("name")}
                >
                  Nombre
                </th>
                <th
                  className="w-[70px] cursor-pointer px-3 font-medium"
                  onClick={() => toggleSort("ext")}
                >
                  Ext
                </th>
                <th
                  className="w-[90px] cursor-pointer px-3 font-medium"
                  onClick={() => toggleSort("size")}
                >
                  Tamaño
                </th>
                <th
                  className="w-[160px] cursor-pointer px-3 font-medium"
                  onClick={() => toggleSort("modified")}
                >
                  Modificado
                </th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => {
                const isSel = selected.includes(entry.absolute_path);
                const isCut =
                  clipboard.mode === "cut" &&
                  clipboard.paths.includes(entry.absolute_path);
                return (
                  <tr
                    key={entry.absolute_path}
                    draggable
                    onDragStart={(e) => onDragStart(e, entry)}
                    className={cn(
                      "h-[40px] border-b border-border/50",
                      isSel ? "bg-selection" : "hover:bg-elevated",
                      isCut && "opacity-50",
                    )}
                    onClick={(e) => onRowClick(e, entry)}
                    onDoubleClick={() => handleOpen(entry)}
                    onContextMenu={(e) => onContextMenu(e, entry)}
                  >
                    <td className="truncate px-3">
                      <span className="inline-flex items-center gap-2">
                        {entry.is_directory ? (
                          <FolderOpen size={14} className="text-accent" />
                        ) : (
                          <span className="inline-block w-[14px]" />
                        )}
                        {entry.name}
                        {entry.is_engineering && (
                          <span className="text-[10px] uppercase text-warn">eng</span>
                        )}
                      </span>
                    </td>
                    <td className="px-3 text-muted">{entry.extension ?? "—"}</td>
                    <td className="px-3 tabular-nums text-muted">
                      {entry.is_directory ? "—" : formatBytes(entry.size_bytes)}
                    </td>
                    <td className="px-3 text-[12px] text-muted">
                      {entry.modified_at
                        ? new Date(entry.modified_at).toLocaleString()
                        : "—"}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {incorporate && currentPath && (
        <IncorporationDialog
          projectId={projectId}
          categoryKey={categoryKey}
          absolutePaths={incorporate.paths}
          destAbsolutePath={
            incorporate.alreadyInProject ? null : currentPath
          }
          preferMove={incorporate.preferMove}
          alreadyInProject={incorporate.alreadyInProject}
          onClose={() => setIncorporate(null)}
          onCompleted={(message, warning) => {
            setIncorporate(null);
            setStatus(message);
            setWarning(warning);
            void refresh();
            void qc.invalidateQueries({ queryKey: ["documents", projectId] });
          }}
        />
      )}

      {contextMenu && (
        <div
          className="fixed z-50 min-w-[200px] border border-border bg-panel py-1 shadow-lg"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <MenuItem
            label="Abrir"
            disabled={!contextMenu.entry}
            onClick={() => {
              if (contextMenu.entry) void handleOpen(contextMenu.entry);
              setContextMenu(null);
            }}
          />
          <MenuItem
            label="Incorporar…"
            disabled={
              selected.filter((p) =>
                entries.some((e) => e.absolute_path === p && !e.is_directory),
              ).length === 0 &&
              !(contextMenu.entry && !contextMenu.entry.is_directory)
            }
            onClick={() => {
              const paths =
                selected.length > 0
                  ? selected.filter((p) =>
                      entries.some(
                        (e) => e.absolute_path === p && !e.is_directory,
                      ),
                    )
                  : contextMenu.entry && !contextMenu.entry.is_directory
                    ? [contextMenu.entry.absolute_path]
                    : [];
              if (paths.length) {
                setIncorporate({ paths, alreadyInProject: true });
              }
              setContextMenu(null);
            }}
          />
          <MenuItem
            label="Revelar en explorador"
            disabled={!contextMenu.entry}
            onClick={() => {
              if (contextMenu.entry)
                void api.revealInExplorer(contextMenu.entry.absolute_path);
              setContextMenu(null);
            }}
          />
          <MenuItem
            label="Copiar"
            onClick={() => {
              void copySelected();
              setContextMenu(null);
            }}
          />
          <MenuItem
            label="Cortar"
            onClick={() => {
              void cutSelected();
              setContextMenu(null);
            }}
          />
          <MenuItem
            label="Pegar"
            disabled={!clipboard.mode}
            onClick={() => {
              void pasteHere();
              setContextMenu(null);
            }}
          />
          <MenuItem
            label="Renombrar"
            onClick={() => {
              void renameSelected();
              setContextMenu(null);
            }}
          />
          <MenuItem
            label="Eliminar"
            onClick={() => {
              void deleteSelected();
              setContextMenu(null);
            }}
          />
          <MenuItem
            label="Propiedades"
            disabled={!contextMenu.entry}
            onClick={() => {
              setInspectorTab("file");
              setContextMenu(null);
            }}
          />
        </div>
      )}
    </div>
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
      className="flex h-[32px] w-full items-center px-3 text-left text-[12.5px] hover:bg-elevated disabled:opacity-40"
      onClick={onClick}
    >
      {label}
    </button>
  );
}
