import { useEffect, useState, type MouseEvent } from "react";
import { Search, Settings, PanelRight } from "lucide-react";
import { Button } from "./ui/button";
import { useUiStore } from "@/stores/ui-store";
import { GlobalSearch } from "./GlobalSearch";
import { WindowControls } from "./WindowControls";
import { isTauriRuntime } from "@/lib/os-drop";

interface TopBarProps {
  rootPath?: string | null;
  indexMessage?: string | null;
}

async function toggleMaximizeWindow() {
  if (!isTauriRuntime()) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().toggleMaximize();
  } catch {
    /* ignore outside Tauri / missing capability */
  }
}

function onDragRegionDoubleClick(e: MouseEvent) {
  // Avoid toggling when the event bubbled from an interactive child.
  if ((e.target as HTMLElement).closest("button, a, input, [role='button']")) {
    return;
  }
  void toggleMaximizeWindow();
}

export function TopBar({ rootPath, indexMessage }: TopBarProps) {
  const setView = useUiStore((s) => s.setView);
  const inspectorOpen = useUiStore((s) => s.inspectorOpen);
  const setInspectorOpen = useUiStore((s) => s.setInspectorOpen);
  const [searchOpen, setSearchOpen] = useState(false);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen(true);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
      <header className="flex h-[42px] shrink-0 items-stretch border-b border-[#1f2937] bg-[#111827] pl-3">
        <div
          className="flex select-none items-center gap-2 pr-2"
          data-tauri-drag-region
          onDoubleClick={onDragRegionDoubleClick}
        >
          <div className="flex h-6 w-6 items-center justify-center rounded-[4px] bg-selection text-[11px] font-bold text-accent">
            IM
          </div>
          <span className="text-[13px] font-semibold tracking-tight">
            IngManager
          </span>
        </div>

        <button
          type="button"
          title="Búsqueda global (Ctrl+K)"
          onClick={() => setSearchOpen(true)}
          data-tauri-drag-region="false"
          className="my-auto ml-4 flex h-[30px] w-[360px] max-w-[40vw] items-center gap-2 rounded-[var(--radius-sm)] border border-[#1f2937] bg-elevated px-2.5 text-left text-[12.5px] text-muted hover:border-accent/50 hover:text-text"
        >
          <Search size={14} />
          <span className="flex-1">Buscar proyectos y documentos…</span>
          <kbd className="rounded border border-[#1f2937] px-1 text-[10px]">
            Ctrl+K
          </kbd>
        </button>

        <div
          className="min-w-2 flex-1"
          data-tauri-drag-region
          onDoubleClick={onDragRegionDoubleClick}
        />

        <div
          className="flex items-center gap-2 pr-1 text-[11.5px] text-muted"
          data-tauri-drag-region="false"
        >
          {(indexMessage || rootPath) && (
            <div
              className="flex max-w-[520px] items-center gap-2 truncate"
              data-tauri-drag-region
              onDoubleClick={onDragRegionDoubleClick}
            >
              {indexMessage && (
                <span className="max-w-[280px] truncate">{indexMessage}</span>
              )}
              {rootPath && (
                <span
                  className="hidden max-w-[320px] truncate lg:inline"
                  title={rootPath}
                >
                  {rootPath}
                </span>
              )}
            </div>
          )}
          <Button
            variant="ghost"
            size="icon"
            title={inspectorOpen ? "Ocultar inspector" : "Mostrar inspector"}
            data-tauri-drag-region="false"
            onClick={() => setInspectorOpen(!inspectorOpen)}
          >
            <PanelRight size={16} />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            title="Ajustes"
            data-tauri-drag-region="false"
            onClick={() => setView({ kind: "settings" })}
          >
            <Settings size={16} />
          </Button>
        </div>

        <WindowControls />
      </header>
      <GlobalSearch open={searchOpen} onClose={() => setSearchOpen(false)} />
    </>
  );
}
