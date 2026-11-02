import { Search, Settings, PanelRight } from "lucide-react";
import { Button } from "./ui/button";
import { useUiStore } from "@/stores/ui-store";

interface TopBarProps {
  rootPath?: string | null;
  indexMessage?: string | null;
}

export function TopBar({ rootPath, indexMessage }: TopBarProps) {
  const setView = useUiStore((s) => s.setView);
  const inspectorOpen = useUiStore((s) => s.inspectorOpen);
  const setInspectorOpen = useUiStore((s) => s.setInspectorOpen);

  return (
    <header className="flex h-[42px] shrink-0 items-center gap-3 border-b border-border bg-panel px-3">
      <div className="flex items-center gap-2">
        <div className="flex h-6 w-6 items-center justify-center rounded-[4px] bg-selection text-[11px] font-bold text-accent">
          IM
        </div>
        <span className="text-[13px] font-semibold tracking-tight">IngManager</span>
      </div>

      <button
        type="button"
        disabled
        title="Búsqueda global (Fase 5)"
        className="ml-4 flex h-[30px] w-[360px] max-w-[40vw] cursor-not-allowed items-center gap-2 rounded-[var(--radius-sm)] border border-border bg-elevated px-2.5 text-left text-[12.5px] text-muted opacity-70"
      >
        <Search size={14} />
        <span className="flex-1">Buscar proyectos y documentos…</span>
        <kbd className="rounded border border-border px-1 text-[10px]">Ctrl+K</kbd>
      </button>

      <div className="ml-auto flex items-center gap-2 text-[11.5px] text-muted">
        {indexMessage && <span className="max-w-[280px] truncate">{indexMessage}</span>}
        {rootPath && (
          <span className="hidden max-w-[320px] truncate lg:inline" title={rootPath}>
            {rootPath}
          </span>
        )}
        <Button
          variant="ghost"
          size="icon"
          title={inspectorOpen ? "Ocultar inspector" : "Mostrar inspector"}
          onClick={() => setInspectorOpen(!inspectorOpen)}
        >
          <PanelRight size={16} />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          title="Ajustes"
          onClick={() => setView({ kind: "settings" })}
        >
          <Settings size={16} />
        </Button>
      </div>
    </header>
  );
}
