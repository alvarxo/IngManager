import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, HardDrive } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "./ui/button";

export function SetupScreen() {
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [path, setPath] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: (p: string) => api.setRootPath(p),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["app-status"] });
      try {
        await api.startInitialIndex();
      } catch {
        // indexing may no-op; status still valid
      }
    },
    onError: (e: Error) => setError(String(e)),
  });

  async function pickFolder() {
    setError(null);
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Seleccionar carpeta raíz de proyectos",
    });
    if (typeof selected === "string") {
      setPath(selected);
    }
  }

  return (
    <div className="flex h-full items-center justify-center bg-bg">
      <div className="w-[520px] border border-border bg-panel p-8">
        <div className="mb-6 flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-[var(--radius-md)] bg-selection text-accent">
            <HardDrive size={20} />
          </div>
          <div>
            <h1 className="text-[18px] font-semibold tracking-tight text-text">
              IngManager
            </h1>
            <p className="text-muted">Configuración inicial</p>
          </div>
        </div>

        <p className="mb-5 text-[13px] text-muted">
          Elija la carpeta raíz donde se almacenarán los proyectos de ingeniería
          (estructura tipo <code className="text-text">INGMANAGER_PROYECTOS</code>).
          La aplicación solo operará bajo esta ruta.
        </p>

        <div className="mb-4 flex items-center gap-2">
          <div className="min-h-[36px] flex-1 truncate border border-border bg-elevated px-3 py-2 text-[12.5px] text-muted">
            {path ?? "Ninguna carpeta seleccionada"}
          </div>
          <Button variant="secondary" onClick={pickFolder}>
            <FolderOpen size={15} />
            Examinar…
          </Button>
        </div>

        {error && (
          <div className="mb-3 border border-error/40 bg-error/10 px-3 py-2 text-[12.5px] text-error">
            {error}
          </div>
        )}

        <div className="flex justify-end">
          <Button
            disabled={!path || mutation.isPending}
            onClick={() => path && mutation.mutate(path)}
          >
            {mutation.isPending ? "Guardando…" : "Continuar"}
          </Button>
        </div>
      </div>
    </div>
  );
}
