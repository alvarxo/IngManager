/** Helpers for Tauri 2 native OS drag-drop (real absolute paths). */

export type OsDropPoint = { x: number; y: number };

/** Convert Tauri physical-pixel position to CSS/DOM coordinates. */
export function physicalToCssPoint(
  position: OsDropPoint,
  devicePixelRatio = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1,
): OsDropPoint {
  const dpr = devicePixelRatio > 0 ? devicePixelRatio : 1;
  return { x: position.x / dpr, y: position.y / dpr };
}

export function pointInElement(
  point: OsDropPoint,
  el: DOMRect | { left: number; top: number; right: number; bottom: number },
): boolean {
  return (
    point.x >= el.left &&
    point.x <= el.right &&
    point.y >= el.top &&
    point.y <= el.bottom
  );
}

export function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}
