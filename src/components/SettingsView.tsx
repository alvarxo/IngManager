import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "./ui/button";
import { ImportWizard } from "./ImportWizard";
import { useUiStore } from "@/stores/ui-store";

export function SettingsView() {
  const qc = useQueryClient();
  const setView = useUiStore((s) => s.setView);
  const [msg, setMsg] = useState<string | null>(null);
  const status = useQuery({
    queryKey: ["app-status"],
    queryFn: () => api.getAppStatus(),
  });

  const setRoot = useMutation({
    mutationFn: (path: string) => api.setRootPath(path),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["app-status"] });
      setMsg("Carpeta raíz actualizada.");
    },
    onError: (e: Error) => setMsg(String(e)),
  });

  const reindex = useMutation({
    mutationFn: () => api.reindexRoot(),
    onSuccess: () => setMsg("Reindexación iniciada…"),
    onError: (e: Error) => setMsg(String(e)),
  });

  const rebuildFts = useMutation({
    mutationFn: () => api.rebuildSearchIndex(),
    onSuccess: (n) => setMsg(`Índice FTS reconstruido: ${n} filas.`),
    onError: (e: Error) => setMsg(String(e)),
  });

  const backup = useMutation({
    mutationFn: async () => {
      const dest = await open({
        directory: true,
        multiple: false,
        title: "Destino del backup portable",
      });
      if (typeof dest !== "string") {
        throw new Error("Cancelado");
      }
      return api.createPortableBackup(dest, true, null);
    },
    onSuccess: (res) =>
      setMsg(
        `Backup listo: ${res.backup_dir}${res.zip_path ? ` · zip ${res.zip_path}` : ""} (${res.project_count} proyectos)`,
      ),
    onError: (e: Error) => {
      if (String(e).includes("Cancelado")) return;
      setMsg(String(e));
    },
  });

  async function changeRoot() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Cambiar carpeta raíz",
    });
    if (typeof selected === "string") {
      setRoot.mutate(selected);
    }
  }

  return (
    <div className="h-full overflow-auto bg-bg p-6">
      <h1 className="mb-1 text-[18px] font-semibold">Ajustes</h1>
      <p className="mb-6 text-[13px] text-muted">
        Configuración local, respaldo, búsqueda e importación.
      </p>

      <section className="mb-6 max-w-2xl border border-border bg-panel p-4">
        <h2 className="mb-3 text-[13px] font-semibold">Carpeta raíz</h2>
        <p className="mb-2 break-all font-mono text-[12.5px] text-muted">
          {status.data?.root_path ?? "—"}
        </p>
        <p className="mb-3 text-[12px] text-muted">
          Acceso: {status.data?.root_valid ? "válido" : "no válido"} · Proyectos:{" "}
          {status.data?.project_count ?? 0}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" onClick={changeRoot}>
            Cambiar carpeta…
          </Button>
          <Button
            variant="secondary"
            onClick={() => reindex.mutate()}
            disabled={reindex.isPending || !status.data?.root_valid}
          >
            Reindexar
          </Button>
        </div>
      </section>

      <section className="mb-6 max-w-2xl border border-border bg-panel p-4">
        <h2 className="mb-2 text-[13px] font-semibold">Base de datos</h2>
        <p className="mb-3 break-all font-mono text-[12px] text-muted">
          {status.data?.db_path ?? "—"}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary"
            onClick={() => rebuildFts.mutate()}
            disabled={rebuildFts.isPending}
          >
            Reconstruir FTS
          </Button>
          <Button
            onClick={() => backup.mutate()}
            disabled={backup.isPending || !status.data?.root_valid}
          >
            {backup.isPending ? "Creando backup…" : "Backup portable…"}
          </Button>
        </div>
        <p className="mt-2 text-[11.5px] text-muted">
          El backup usa VACUUM INTO (snapshot consistente), manifiesto JSON y
          copia opcional de proyectos + ZIP.
        </p>
      </section>

      <section className="mb-6 max-w-2xl border border-border bg-panel p-4">
        <h2 className="mb-3 text-[13px] font-semibold">
          Importar proyecto existente
        </h2>
        <ImportWizard
          onDone={(projectId) => {
            setView({ kind: "project", projectId });
          }}
        />
      </section>

      {msg && (
        <div className="mt-4 max-w-2xl border border-border bg-elevated px-3 py-2 text-[12.5px]">
          {msg}
        </div>
      )}
    </div>
  );
}
