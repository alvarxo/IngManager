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
      await qc.invalidateQueries({ queryKey: ["authorized-roots"] });
      setMsg("Carpeta raíz principal actualizada.");
    },
    onError: (e: Error) => setMsg(String(e)),
  });

  const addRoot = useMutation({
    mutationFn: (path: string) => api.addAuthorizedRoot(path, null, false),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["app-status"] });
      await qc.invalidateQueries({ queryKey: ["authorized-roots"] });
      setMsg("Raíz autorizada añadida.");
    },
    onError: (e: Error) => setMsg(String(e)),
  });

  const removeRoot = useMutation({
    mutationFn: (id: string) => api.removeAuthorizedRoot(id),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["app-status"] });
      await qc.invalidateQueries({ queryKey: ["authorized-roots"] });
      setMsg("Raíz eliminada.");
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
      title: "Cambiar carpeta raíz principal",
    });
    if (typeof selected === "string") {
      setRoot.mutate(selected);
    }
  }

  async function addExtraRoot() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Añadir raíz autorizada",
    });
    if (typeof selected === "string") {
      addRoot.mutate(selected);
    }
  }

  const roots = status.data?.authorized_roots ?? [];

  return (
    <div className="h-full overflow-auto bg-bg p-6">
      <h1 className="mb-1 text-[18px] font-semibold">Ajustes</h1>
      <p className="mb-6 text-[13px] text-muted">
        Raíces autorizadas, respaldo, búsqueda e importación flexible.
      </p>

      <section className="mb-6 max-w-2xl border border-border bg-panel p-4">
        <h2 className="mb-3 text-[13px] font-semibold">Raíces autorizadas</h2>
        <p className="mb-2 break-all font-mono text-[12.5px] text-muted">
          Principal: {status.data?.root_path ?? "—"}
        </p>
        <p className="mb-3 text-[12px] text-muted">
          Acceso: {status.data?.root_valid ? "válido" : "no válido"} · Proyectos:{" "}
          {status.data?.project_count ?? 0}
        </p>
        {roots.length > 0 && (
          <ul className="mb-3 space-y-1">
            {roots.map((r) => (
              <li
                key={r.id}
                className="flex items-start justify-between gap-2 border border-border/60 px-2 py-1.5 text-[12px]"
              >
                <div className="min-w-0">
                  <div className="font-medium">
                    {r.label ?? "Raíz"}
                    {r.is_primary === 1 ? " · principal" : ""}
                  </div>
                  <div className="break-all font-mono text-[11px] text-muted">
                    {r.absolute_path}
                  </div>
                </div>
                {r.is_primary !== 1 && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => removeRoot.mutate(r.id)}
                    disabled={removeRoot.isPending}
                  >
                    Quitar
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" onClick={changeRoot}>
            Cambiar raíz principal…
          </Button>
          <Button variant="secondary" onClick={() => void addExtraRoot()}>
            Añadir raíz…
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
          copia de proyectos por su ruta física real + ZIP.
        </p>
      </section>

      <section className="mb-6 max-w-2xl border border-border bg-panel p-4">
        <h2 className="mb-2 text-[13px] font-semibold">Diagnósticos</h2>
        <p className="mb-3 text-[12px] text-muted">
          Registro persistente de aperturas de proyecto y reconcile en segundo
          plano (útil si un proyecto como 1090 fallaba al abrir).
        </p>
        <Button
          variant="secondary"
          onClick={async () => {
            try {
              await api.openDiagnosticsFolder();
              setMsg("Carpeta de diagnósticos abierta.");
            } catch (e) {
              setMsg(String(e));
            }
          }}
        >
          Abrir carpeta de diagnósticos
        </Button>
      </section>

      <section className="mb-6 max-w-2xl border border-border bg-panel p-4">
        <h2 className="mb-3 text-[13px] font-semibold">
          Añadir / importar proyecto existente
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
