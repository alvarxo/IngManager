import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type FocusEvent,
} from "react";
import { Menu, PanelLeftClose } from "lucide-react";
import { cn } from "@/lib/utils";
import { NavRail } from "./NavRail";
import { ProjectList } from "./ProjectList";

const PIN_KEY = "ingmanager_sidebar_pinned";
const CLOSE_DELAY_MS = 200;

function readPinned(): boolean {
  try {
    return localStorage.getItem(PIN_KEY) === "1";
  } catch {
    return false;
  }
}

function writePinned(value: boolean) {
  try {
    localStorage.setItem(PIN_KEY, value ? "1" : "0");
  } catch {
    /* ignore quota / private mode */
  }
}

/**
 * Left chrome: narrow icon rail + ProjectList flyout.
 * - Default collapsed (main content gets the width).
 * - Hover / focus on rail or panel → open overlay (~200ms anti-flicker on leave).
 * - Click hamburger → pin toggle (persisted in localStorage).
 */
export function LeftSidebar() {
  const panelId = useId();
  const [pinned, setPinned] = useState(readPinned);
  const [hoverOpen, setHoverOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  const open = pinned || hoverOpen || busy;

  const clearCloseTimer = useCallback(() => {
    if (closeTimer.current != null) {
      clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
  }, []);

  const scheduleClose = useCallback(() => {
    clearCloseTimer();
    closeTimer.current = setTimeout(() => {
      setHoverOpen(false);
      closeTimer.current = null;
    }, CLOSE_DELAY_MS);
  }, [clearCloseTimer]);

  const openNow = useCallback(() => {
    clearCloseTimer();
    setHoverOpen(true);
  }, [clearCloseTimer]);

  useEffect(() => () => clearCloseTimer(), [clearCloseTimer]);

  useEffect(() => {
    writePinned(pinned);
  }, [pinned]);

  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      if (busy) return;
      if (pinned) setPinned(false);
      setHoverOpen(false);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, busy, pinned]);

  function togglePin() {
    setPinned((v) => !v);
    openNow();
  }

  function onRootBlur(e: FocusEvent<HTMLDivElement>) {
    const next = e.relatedTarget as Node | null;
    if (next && rootRef.current?.contains(next)) return;
    if (pinned || busy) return;
    scheduleClose();
  }

  return (
    <div
      ref={rootRef}
      className="relative z-30 flex h-full shrink-0"
      onMouseEnter={openNow}
      onMouseLeave={() => {
        if (pinned || busy) return;
        scheduleClose();
      }}
      onFocusCapture={openNow}
      onBlurCapture={onRootBlur}
    >
      <div className="flex w-11 shrink-0 flex-col border-r border-border bg-panel">
        <div className="flex h-[42px] items-center justify-center border-b border-border">
          <button
            type="button"
            className={cn(
              "flex h-8 w-8 items-center justify-center rounded-[var(--radius-sm)] text-muted",
              "hover:bg-elevated hover:text-text",
              open && "bg-selection text-text",
            )}
            aria-label={
              pinned
                ? "Desanclar panel de proyectos"
                : "Anclar panel de proyectos"
            }
            aria-expanded={open}
            aria-controls={panelId}
            title={
              pinned
                ? "Desanclar panel (clic). Hover también abre."
                : "Anclar panel abierto (clic). Hover abre temporalmente."
            }
            onClick={togglePin}
          >
            {pinned ? <PanelLeftClose size={18} /> : <Menu size={18} />}
          </button>
        </div>
        <NavRail compact />
      </div>

      <div
        id={panelId}
        role="region"
        aria-label="Proyectos"
        aria-hidden={!open}
        className={cn(
          "absolute left-11 top-0 flex h-full w-[280px] flex-col bg-panel shadow-[4px_0_16px_rgba(0,0,0,0.35)]",
          "origin-left transition-[opacity,transform] duration-150 ease-out",
          open
            ? "pointer-events-auto translate-x-0 opacity-100"
            : "pointer-events-none -translate-x-1 opacity-0",
        )}
      >
        <ProjectList onBusyChange={setBusy} />
      </div>
    </div>
  );
}
