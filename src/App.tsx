import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import type { IndexProgress } from "@/lib/types";
import { useUiStore } from "@/stores/ui-store";
import { SetupScreen } from "@/components/SetupScreen";
import { AppShell } from "@/components/AppShell";
import { UndecoratedFrame } from "@/components/WindowChrome";

export default function App() {
  const qc = useQueryClient();
  const indexMessage = useUiStore((s) => s.indexMessage);
  const setIndexMessage = useUiStore((s) => s.setIndexMessage);

  const status = useQuery({
    queryKey: ["app-status"],
    queryFn: () => api.getAppStatus(),
  });

  useEffect(() => {
    let unlistenIndex: (() => void) | undefined;
    let unlistenFs: (() => void) | undefined;

    listen<IndexProgress>("index-progress", (event) => {
      const p = event.payload;
      setIndexMessage(p.message ?? `${p.phase}: ${p.scanned}`);
      if (p.done) {
        setTimeout(() => setIndexMessage(null), 4000);
        void qc.invalidateQueries({ queryKey: ["fm-entries"] });
        void qc.invalidateQueries({ queryKey: ["categories"] });
      }
    }).then((fn) => {
      unlistenIndex = fn;
    });

    listen<IndexProgress>("fs-changed", (event) => {
      const p = event.payload;
      if (p.done) {
        void qc.invalidateQueries({ queryKey: ["fm-entries"] });
        void qc.invalidateQueries({ queryKey: ["categories"] });
        void qc.invalidateQueries({ queryKey: ["entry-props"] });
        if (p.message) {
          setIndexMessage(p.message);
          setTimeout(() => setIndexMessage(null), 2500);
        }
      }
    }).then((fn) => {
      unlistenFs = fn;
    });

    return () => {
      unlistenIndex?.();
      unlistenFs?.();
    };
  }, [setIndexMessage, qc]);

  useEffect(() => {
    if (status.data?.root_valid) {
      api.startInitialIndex().catch(() => undefined);
      api.startFileWatcher().catch(() => undefined);
    }
  }, [status.data?.root_valid]);

  if (status.isLoading) {
    return (
      <UndecoratedFrame>
        <div className="flex h-full items-center justify-center bg-[#111827] text-muted">
          Iniciando IngManager…
        </div>
      </UndecoratedFrame>
    );
  }

  if (status.error) {
    return (
      <UndecoratedFrame>
        <div className="flex h-full items-center justify-center bg-[#111827] text-error">
          Error al iniciar: {String(status.error)}
        </div>
      </UndecoratedFrame>
    );
  }

  if (!status.data?.root_valid) {
    return (
      <UndecoratedFrame>
        <SetupScreen />
      </UndecoratedFrame>
    );
  }

  return (
    <AppShell rootPath={status.data.root_path} indexMessage={indexMessage} />
  );
}
