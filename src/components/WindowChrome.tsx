import type { MouseEvent, ReactNode } from "react";
import { WindowControls } from "./WindowControls";
import { isTauriRuntime } from "@/lib/os-drop";

async function toggleMaximizeWindow() {
  if (!isTauriRuntime()) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().toggleMaximize();
  } catch {
    /* ignore */
  }
}

function onDragRegionDoubleClick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest("button, a, input, [role='button']")) {
    return;
  }
  void toggleMaximizeWindow();
}

/** Thin chrome for boot/setup screens that do not render TopBar. */
export function UndecoratedFrame({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full flex-col bg-[#111827]">
      <div className="flex h-[42px] shrink-0 items-stretch border-b border-[#1f2937]">
        <div
          className="min-w-0 flex-1 select-none"
          data-tauri-drag-region
          onDoubleClick={onDragRegionDoubleClick}
        />
        <WindowControls />
      </div>
      <div className="min-h-0 flex-1">{children}</div>
    </div>
  );
}
