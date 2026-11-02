import { useEffect, useRef, type RefObject } from "react";
import { isTauriRuntime, physicalToCssPoint, pointInElement } from "@/lib/os-drop";

export interface NativeFileDropOptions {
  /** Called with absolute OS paths when dropped over the target element. */
  onDrop: (paths: string[]) => void;
  onActiveChange?: (active: boolean) => void;
  enabled?: boolean;
  /**
   * When true (default), Shift held at drop time is reported via `preferMove`.
   * File Manager uses this to preselect move; Inbox ignores it.
   */
  onDropWithModifiers?: (paths: string[], preferMove: boolean) => void;
}

/**
 * Listen to Tauri 2 `onDragDropEvent` and route drops that hit `targetRef`.
 * Keeps HTML5 internal DnD (`application/x-ingmanager-paths`) untouched —
 * OS drops use this channel; with `dragDropEnabled: true` DOM file drops
 * do not expose real paths on Windows.
 */
export function useNativeFileDrop(
  targetRef: RefObject<HTMLElement | null>,
  options: NativeFileDropOptions,
) {
  const optsRef = useRef(options);
  optsRef.current = options;
  const preferMoveRef = useRef(false);

  useEffect(() => {
    if (options.enabled === false) return;
    if (!isTauriRuntime()) return;

    let unlisten: (() => void) | undefined;
    let cancelled = false;

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Shift") preferMoveRef.current = true;
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === "Shift") preferMoveRef.current = false;
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);

    void (async () => {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        const un = await getCurrentWebview().onDragDropEvent((event) => {
          const el = targetRef.current;
          if (!el) return;
          const opts = optsRef.current;
          const payload = event.payload;

          if (payload.type === "leave") {
            opts.onActiveChange?.(false);
            return;
          }

          if (payload.type === "enter" || payload.type === "over") {
            const css = physicalToCssPoint(payload.position);
            const hit = pointInElement(css, el.getBoundingClientRect());
            opts.onActiveChange?.(hit);
            return;
          }

          if (payload.type === "drop") {
            const css = physicalToCssPoint(payload.position);
            const hit = pointInElement(css, el.getBoundingClientRect());
            opts.onActiveChange?.(false);
            if (!hit) return;
            const paths = payload.paths.filter(Boolean);
            if (paths.length === 0) return;
            const preferMove = preferMoveRef.current;
            if (opts.onDropWithModifiers) {
              opts.onDropWithModifiers(paths, preferMove);
            } else {
              opts.onDrop(paths);
            }
          }
        });
        if (cancelled) {
          un();
        } else {
          unlisten = un;
        }
      } catch {
        // Not running inside a Tauri webview (vitest / browser preview).
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      optsRef.current.onActiveChange?.(false);
    };
  }, [targetRef, options.enabled]);
}
