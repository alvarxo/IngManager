import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { RepairModelHistoryPreview } from "@/lib/types";
import { Button } from "./ui/button";

interface Props {
  projectId: string;
  documentId: string | null;
  onClose: () => void;
  onCompleted: (message: string) => void;
}

export function RepairModelHistoryDialog({
  projectId,
  documentId,
  onClose,
  onCompleted,
}: Props) {
  const [preview, setPreview] = useState<RepairModelHistoryPreview | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setBusy(true);
    setError(null);
    setPreview(null);
    void api
      .previewRepairModelHistory({
        project_id: projectId,
        document_id: documentId,
      })
      .then((p) => {
        if (!cancelled) setPreview(p);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, documentId]);

  async function apply() {
    if (!preview || preview.versions_to_drop === 0) return;
    if (
      !window.confirm(
        `¿Reparar historial?\n\nSe eliminarán ${preview.versions_to_drop} versión(es) falsa(s) (p.ej. .sbk/aux) de la base de datos.\nSe crea un backup SQLite. Los archivos físicos NO se borran.`,
      )
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const r = await api.applyRepairModelHistory({
        project_id: projectId,
        document_id: documentId,
      });
      onCompleted(
        `Historial reparado: ${r.versions_removed} versión(es) quitada(s), ${r.versions_kept} conservada(s). Backup: ${r.db_backup_path}`,
      );
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div className="flex max-h-[90vh] w-full max-w-2xl flex-col gap-3 overflow-hidden rounded border border-border bg-panel p-4 shadow-lg">
        <div className="flex items-start justify-between gap-2">
          <div>
            <h2 className="text-[14px] font-semibold">Reparar historial…</h2>
            <p className="mt-1 text-[12px] text-muted">
              Conserva solo versiones con `.sdb`. Quita versiones falsas creadas
              por `.sbk`/auxiliares. No borra archivos en disco.
            </p>
          </div>
          <Button size="sm" variant="secondary" onClick={onClose}>
            Cerrar
          </Button>
        </div>

        {busy && !preview && (
          <p className="text-[12px] text-muted">Calculando preview…</p>
        )}
        {error && (
          <div className="border border-error/40 bg-error/10 px-2 py-1.5 text-[12px] text-error">
            {error}
          </div>
        )}

        {preview && (
          <div className="min-h-0 flex-1 space-y-3 overflow-auto text-[12px]">
            <p>
              Total: {preview.total_versions_before} →{" "}
              {preview.total_versions_after} versiones (
              {preview.versions_to_drop} a quitar)
            </p>
            {preview.warnings.map((w) => (
              <p key={w} className="text-amber-700">
                {w}
              </p>
            ))}
            {preview.documents.map((d) => (
              <div
                key={d.document_id}
                className="rounded border border-border/60 p-2"
              >
                <div className="font-medium">
                  {d.business_code?.trim() || d.title}{" "}
                  <span className="font-normal text-muted">
                    ({d.versions_before} → {d.versions_after})
                  </span>
                </div>
                <ul className="mt-1 space-y-0.5">
                  {d.decisions.map((v) => (
                    <li
                      key={v.version_id}
                      className={
                        v.keep ? "text-foreground" : "text-muted line-through"
                      }
                    >
                      v{String(v.version_number).padStart(2, "0")}{" "}
                      {v.principal_file_name ?? "(sin archivo)"} — {v.reason}
                      {v.keep && v.new_version_number != null
                        ? ` → v${String(v.new_version_number).padStart(2, "0")}`
                        : ""}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}

        <div className="flex justify-end gap-2 border-t border-border pt-2">
          <Button size="sm" variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button
            size="sm"
            disabled={
              busy || !preview || preview.versions_to_drop === 0 || !!error
            }
            onClick={() => void apply()}
          >
            {busy ? "Procesando…" : "Aplicar reparación"}
          </Button>
        </div>
      </div>
    </div>
  );
}
