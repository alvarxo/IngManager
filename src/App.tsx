import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import type { IndexProgress } from "@/lib/types";
import { useUiStore } from "@/stores/ui-store";
import { SetupScreen } from "@/components/SetupScreen";
import { AppShell } from "@/components/AppShell";

export default function App() {
  const indexMessage = useUiStore((s) => s.indexMessage);
  const setIndexMessage = useUiStore((s) => s.setIndexMessage);

  const status = useQuery({
    queryKey: ["app-status"],
    queryFn: () => api.getAppStatus(),
  });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<IndexProgress>("index-progress", (event) => {
      const p = event.payload;
      setIndexMessage(p.message ?? `${p.phase}: ${p.scanned}`);
      if (p.done) {
        setTimeout(() => setIndexMessage(null), 4000);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [setIndexMessage]);

  useEffect(() => {
    if (status.data?.root_valid) {
      api.startInitialIndex().catch(() => undefined);
    }
  }, [status.data?.root_valid]);

  if (status.isLoading) {
    return (
      <div className="flex h-full items-center justify-center bg-bg text-muted">
        Iniciando IngManager…
      </div>
    );
  }

  if (status.error) {
    return (
      <div className="flex h-full items-center justify-center bg-bg text-error">
        Error al iniciar: {String(status.error)}
      </div>
    );
  }

  if (!status.data?.root_valid) {
    return <SetupScreen />;
  }

  return (
    <AppShell rootPath={status.data.root_path} indexMessage={indexMessage} />
  );
}
