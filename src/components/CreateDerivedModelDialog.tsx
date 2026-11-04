import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { DerivedModelPreview, DocumentRow } from "@/lib/types";
import { Button } from "./ui/button";

interface Props {
  sourceDocument: DocumentRow;
  onClose: () => void;
  onCompleted: (message: string) => void;
}

export function CreateDerivedModelDialog({
  sourceDocument,
  onClose,
  onCompleted,
}: Props) {
  const [title, setTitle] = useState(`${sourceDocument.title} (derivado)`);
  const [copyCalc, setCopyCalc] = useState(true);
  const [preview, setPreview] = useState<DerivedModelPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setBusy(true);
    setError(null);
    void api
      .previewCreateDerivedModel(sourceDocument.id, title, copyCalc)
      .then((p) => {
        if (!cancelled) setPreview(p);
      })
      .catch((e) => {
        if (!cancelled) {
          setPreview(null);
          setError(String(e));
        }
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [sourceDocument.id, title, copyCalc]);

  async function apply() {
    setBusy(true);
    setError(null);
    try {
      const r = await api.createDerivedModel({
        source_document_id: sourceDocument.id,
        title: title.trim() || null,
        copy_calc_sheets: copyCalc,
        notes: null,
      });
      const code = r.business_code ?? r.document_id.slice(0, 8);
      const sheets =
        r.copied_calc_absolute_paths.length > 0
          ? `\nPlanillas calc copiadas: ${r.copied_calc_absolute_paths.length}`
          : "";
      onCompleted(
        `Modelo derivado creado: ${code} «${r.title}».\nSDB: ${r.copied_sdb_absolute_path}${sheets}\nRelación deriva_de registrada.`,
      );
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  const originLabel =
    sourceDocument.business_code?.trim() || sourceDocument.title;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div className="max-h-[90vh] w-full max-w-lg overflow-auto border border-border bg-panel shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold">Crear modelo derivado</h2>
          <p className="mt-1 text-[12px] text-muted">
            Copia el .sdb actual a un archivo nuevo (stem distinto), asigna UUID
            y código MOD nuevos, y registra provenance{" "}
            <span className="font-mono">deriva_de</span>. No fusiona con el
            origen ni promociona planillas.
          </p>
        </div>

        <div className="space-y-3 px-4 py-3 text-[12.5px]">
          <div>
            <div className="text-[11px] uppercase text-muted">Origen</div>
            <div className="font-medium">{originLabel}</div>
            {preview && (
              <div className="mt-0.5 break-all font-mono text-[11px] text-muted">
                {preview.source_sdb_absolute_path}
              </div>
            )}
          </div>

          <label className="block">
            <span className="text-[11px] uppercase text-muted">Título</span>
            <input
              className="mt-1 w-full border border-border bg-elevated px-2 py-1.5 text-[12.5px]"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </label>

          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={copyCalc}
              onChange={(e) => setCopyCalc(e.target.checked)}
            />
            Copiar planillas calc a archivos nuevos (nunca compartir path)
          </label>

          {preview && (
            <div className="space-y-1 border border-border/60 bg-elevated px-2 py-2 text-[12px]">
              <div>
                Código propuesto:{" "}
                <span className="font-mono font-semibold">
                  {preview.proposed_business_code}
                </span>
              </div>
              <div>
                Destino SDB:{" "}
                <span className="break-all font-mono text-[11px]">
                  {preview.destination_sdb_file_name}
                </span>
              </div>
              <div className="text-muted">
                group_key: {preview.source_group_key} →{" "}
                {preview.proposed_group_key}
              </div>
              {copyCalc && (
                <div className="text-muted">
                  Calc a copiar: {preview.calc_sources.length}
                  {preview.calc_destinations[0]
                    ? ` → ${preview.calc_destinations[0].split(/[/\\]/).pop()}`
                    : preview.calc_sources.length === 0
                      ? " (ninguna asociada)"
                      : ""}
                </div>
              )}
            </div>
          )}

          {error && (
            <div className="border border-error/40 bg-error/10 px-2 py-1.5 text-[12px] text-error">
              {error}
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" onClick={onClose} disabled={busy}>
            Cancelar
          </Button>
          <Button
            disabled={busy || !preview || !title.trim()}
            onClick={() => void apply()}
          >
            {busy ? "Creando…" : "Crear derivado"}
          </Button>
        </div>
      </div>
    </div>
  );
}
