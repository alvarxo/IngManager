import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Star, Clock } from "lucide-react";
import { api } from "@/lib/api";
import { openProjectSafe } from "@/lib/open-project";
import { useUiStore } from "@/stores/ui-store";

export function HomeView() {
  const setView = useUiStore((s) => s.setView);
  const setSelectedDocumentId = useUiStore((s) => s.setSelectedDocumentId);
  const setInspectorTab = useUiStore((s) => s.setInspectorTab);
  const [openError, setOpenError] = useState<string | null>(null);

  async function openById(projectId: string, label: string) {
    setOpenError(null);
    setView({ kind: "project", projectId });
    const result = await openProjectSafe(projectId);
    if (!result.ok) {
      setOpenError(`No se pudo abrir ${label}: ${result.error}`);
    }
  }

  const recent = useQuery({
    queryKey: ["recent-projects"],
    queryFn: () => api.listRecentProjects(8),
  });
  const favorites = useQuery({
    queryKey: ["favorite-projects"],
    queryFn: () => api.listFavoriteProjects(),
  });
  const favoriteDocs = useQuery({
    queryKey: ["favorite-documents", "home"],
    queryFn: () => api.listFavoriteDocuments(null),
  });

  return (
    <div className="flex h-full flex-col overflow-auto bg-bg p-6">
      <div className="mb-6">
        <h1 className="text-[20px] font-semibold tracking-tight">Inicio</h1>
        <p className="text-[13px] text-muted">
          Acceso rápido a proyectos recientes y favoritos.
        </p>
        {openError && (
          <div className="mt-3 border border-error/40 bg-error/10 px-3 py-2 text-[12.5px] text-error">
            {openError}
          </div>
        )}
      </div>

      <section className="mb-8">
        <h2 className="mb-2 flex items-center gap-2 text-[12px] font-semibold uppercase tracking-wide text-muted">
          <Clock size={14} /> Recientes
        </h2>
        {(recent.data?.length ?? 0) === 0 ? (
          <p className="text-[12.5px] text-muted">
            Aún no hay proyectos abiertos. Cree o seleccione uno en la lista.
          </p>
        ) : (
          <table className="w-full max-w-3xl border-collapse text-left">
            <thead>
              <tr className="border-b border-border text-[11px] uppercase text-muted">
                <th className="h-[32px] px-2 font-medium">Código</th>
                <th className="px-2 font-medium">Nombre</th>
                <th className="px-2 font-medium">Cliente</th>
              </tr>
            </thead>
            <tbody>
              {recent.data?.map((p) => (
                <tr
                  key={p.id}
                  className="h-[40px] cursor-pointer border-b border-border/50 hover:bg-elevated"
                  onClick={() =>
                    void openById(p.id, p.code ?? p.name)
                  }
                >
                  <td className="px-2 font-mono text-accent">{p.code ?? "—"}</td>
                  <td className="px-2">{p.name}</td>
                  <td className="px-2 text-muted">{p.client_name ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="mb-8">
        <h2 className="mb-2 flex items-center gap-2 text-[12px] font-semibold uppercase tracking-wide text-muted">
          <Star size={14} /> Proyectos favoritos
        </h2>
        {(favorites.data?.length ?? 0) === 0 ? (
          <p className="text-[12.5px] text-muted">
            Marque proyectos como favoritos desde el inspector.
          </p>
        ) : (
          <ul className="max-w-3xl space-y-1">
            {favorites.data?.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  className="flex h-[40px] w-full items-center gap-3 rounded-[var(--radius-sm)] px-2 hover:bg-elevated"
                  onClick={() => void openById(p.id, p.code ?? p.name)}
                >
                  <span className="font-mono text-accent">{p.code ?? "—"}</span>
                  <span>{p.name}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section>
        <h2 className="mb-2 flex items-center gap-2 text-[12px] font-semibold uppercase tracking-wide text-muted">
          <Star size={14} /> Documentos favoritos
        </h2>
        {(favoriteDocs.data?.length ?? 0) === 0 ? (
          <p className="text-[12.5px] text-muted">
            Marque documentos desde el inspector (estrella en Detalle).
          </p>
        ) : (
          <ul className="max-w-3xl space-y-1">
            {favoriteDocs.data?.map((d) => (
              <li key={d.id}>
                <button
                  type="button"
                  className="flex h-[40px] w-full items-center gap-3 rounded-[var(--radius-sm)] px-2 hover:bg-elevated"
                  onClick={() => {
                    void (async () => {
                      setSelectedDocumentId(d.id);
                      setInspectorTab("doc-detail");
                      setView({
                        kind: "project",
                        projectId: d.project_id,
                        categoryKey: d.category_key ?? undefined,
                      });
                      const result = await openProjectSafe(d.project_id);
                      if (!result.ok) {
                        setOpenError(
                          `No se pudo abrir el proyecto del documento: ${result.error}`,
                        );
                      }
                    })();
                  }}
                >
                  <span className="font-mono text-accent">
                    {d.business_code ?? "—"}
                  </span>
                  <span className="truncate">{d.title}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
