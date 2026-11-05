import { useEffect, useState } from "react";
import { Minus, Square, Copy, X } from "lucide-react";
import { isTauriRuntime } from "@/lib/os-drop";

async function getAppWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauriRuntime()) return;

    let disposed = false;
    const unlisteners: Array<() => void> = [];

    async function syncMaximized() {
      try {
        const win = await getAppWindow();
        const value = await win.isMaximized();
        if (!disposed) setMaximized(value);
      } catch {
        // Browser / missing capability — keep previous state.
      }
    }

    void (async () => {
      await syncMaximized();
      try {
        const win = await getAppWindow();
        unlisteners.push(await win.onResized(() => void syncMaximized()));
        unlisteners.push(await win.onMoved(() => void syncMaximized()));
      } catch {
        // Events unavailable outside Tauri.
      }
    })();

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  async function minimize() {
    if (!isTauriRuntime()) return;
    try {
      await (await getAppWindow()).minimize();
    } catch {
      /* ignore */
    }
  }

  async function toggleMaximize() {
    if (!isTauriRuntime()) return;
    try {
      const win = await getAppWindow();
      await win.toggleMaximize();
      setMaximized(await win.isMaximized());
    } catch {
      /* ignore */
    }
  }

  async function close() {
    if (!isTauriRuntime()) return;
    try {
      await (await getAppWindow()).close();
    } catch {
      /* ignore */
    }
  }

  const btn =
    "inline-flex h-[42px] w-[46px] shrink-0 items-center justify-center text-muted transition-colors hover:bg-elevated hover:text-text";

  return (
    <div
      className="ml-1 flex h-full shrink-0 items-stretch border-l border-[#1f2937]"
      data-tauri-drag-region="false"
    >
      <button
        type="button"
        title="Minimizar"
        aria-label="Minimizar"
        className={btn}
        data-tauri-drag-region="false"
        onClick={() => void minimize()}
      >
        <Minus size={14} strokeWidth={1.75} />
      </button>
      <button
        type="button"
        title={maximized ? "Restaurar" : "Maximizar"}
        aria-label={maximized ? "Restaurar" : "Maximizar"}
        className={btn}
        data-tauri-drag-region="false"
        onClick={() => void toggleMaximize()}
      >
        {maximized ? (
          <Copy size={12} strokeWidth={1.75} />
        ) : (
          <Square size={12} strokeWidth={1.75} />
        )}
      </button>
      <button
        type="button"
        title="Cerrar"
        aria-label="Cerrar"
        className={`${btn} hover:bg-[#7f1d1d]/80 hover:text-[#fecaca]`}
        data-tauri-drag-region="false"
        onClick={() => void close()}
      >
        <X size={14} strokeWidth={1.75} />
      </button>
    </div>
  );
}
