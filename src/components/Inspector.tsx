import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Star } from "lucide-react";
import { api } from "@/lib/api";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";

export function Inspector() {
  const view = useUiStore((s) => s.view);
  const qc = useQueryClient();

  const projectId = view.kind === "project" ? view.projectId : null;
  const project = useQuery({
    queryKey: ["project", projectId],
    queryFn: () => api.getProject(projectId!),
    enabled: !!projectId,
  });

  const favMutation = useMutation({
    mutationFn: (favorite: boolean) =>
      api.updateProject({ id: projectId!, is_favorite: favorite }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["favorite-projects"] });
    },
  });

  const archiveMutation = useMutation({
    mutationFn: () => api.archiveProject(projectId!),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["project", projectId] });
    },
  });

  if (!projectId) {
    return (
      <aside className="flex h-full w-full min-w-0 flex-col border-l border-border bg-panel">
        <div className="border-b border-border px-3 py-3 text-[12px] font-semibold uppercase tracking-wide text-muted">
          Inspector
        </div>
        <div className="p-4 text-[12.5px] text-muted">
          Seleccione un proyecto para ver detalles.
        </div>
      </aside>
    );
  }

  if (project.isLoading || !project.data) {
    return (
      <aside className="h-full w-full min-w-0 border-l border-border bg-panel p-4 text-muted">
        Cargando…
      </aside>
    );
  }

  const p = project.data;
  const isFav = p.is_favorite === 1;

  return (
    <aside className="flex h-full w-full min-w-0 flex-col border-l border-border bg-panel">
      <div className="border-b border-border px-3 py-3 text-[12px] font-semibold uppercase tracking-wide text-muted">
        Inspector
      </div>
      <div className="space-y-4 overflow-auto p-4">
        <div>
          <div className="font-mono text-[15px] font-semibold text-accent">
            {p.code}
          </div>
          <div className="text-[15px] font-medium">{p.name}</div>
        </div>

        <Meta label="Cliente" value={p.client_name ?? "—"} />
        <Meta label="Estado" value={p.status} />
        <Meta label="Ruta" value={p.root_relative_path} mono />
        <Meta label="Inicio" value={p.start_date ?? "—"} />
        <Meta label="Fin" value={p.end_date ?? "—"} />
        <Meta
          label="Descripción"
          value={p.description?.trim() ? p.description : "—"}
        />
        <Meta label="Creado" value={new Date(p.created_at).toLocaleString()} />
        <Meta
          label="Última apertura"
          value={
            p.last_opened_at
              ? new Date(p.last_opened_at).toLocaleString()
              : "—"
          }
        />

        <div className="flex flex-wrap gap-2 pt-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => favMutation.mutate(!isFav)}
          >
            <Star
              size={14}
              className={isFav ? "text-warn" : undefined}
              fill={isFav ? "currentColor" : "none"}
            />
            {isFav ? "Quitar favorito" : "Favorito"}
          </Button>
          {p.status === "active" && (
            <Button
              variant="danger"
              size="sm"
              onClick={() => archiveMutation.mutate()}
              disabled={archiveMutation.isPending}
            >
              Archivar
            </Button>
          )}
        </div>

        <p className="text-[11.5px] text-muted">
          Edición avanzada, versiones y transmisiones llegan en fases
          posteriores. Acciones no implementadas no se muestran como activas.
        </p>
      </div>
    </aside>
  );
}

function Meta({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div>
      <div className="text-[11px] font-medium uppercase tracking-wide text-muted">
        {label}
      </div>
      <div className={mono ? "font-mono text-[12.5px]" : "text-[13px]"}>
        {value}
      </div>
    </div>
  );
}
