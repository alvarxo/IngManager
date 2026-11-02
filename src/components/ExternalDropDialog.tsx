import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Folder, File } from "lucide-react";
import { api } from "@/lib/api";
import type {
  ConflictPolicy,
  ExternalDropMode,
  ExternalDropPreview,
  ExternalDropProgress,
} from "@/lib/types";
import { formatBytes } from "@/lib/utils";
import { Button } from "./ui/button";

interface Props {
  destDir: string;
  sourcePaths: string[];
  preferMove?: boolean;
  onClose: () => void;
  onCompleted: (message: string | null, warning: string | null) => void;
}

export function ExternalDropDialog({
  destDir,
  sourcePaths,
  preferMove = false,
  onClose,
  onCompleted,
}: Props) {
  const [preview, setPreview] = useState<ExternalDropPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<ExternalDropMode>(
    preferMove ? "move" : "copy",
  );
  const [conflictPolicy, setConflictPolicy] =
    useState<ConflictPolicy>("rename");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ExternalDropProgress | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api
      .previewExternalDrop(sourcePaths, destDir, conflictPolicy)
      .then((p) => {
        if (!cancelled) setPreview(p);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [sourcePaths, destDir, conflictPolicy]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<ExternalDropProgress>("external-drop-progress", (event) => {
      setProgress(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  async function confirm() {
    setBusy(true);
    setError(null);
    try {
      const result = await api.applyExternalDrop(
        sourcePaths,
        destDir,
        mode,
        conflictPolicy,
      );
      onCompleted(result.message, result.warning);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50 p-4">
      <div className="flex max-h-[85vh] w-full max-w-[560px] flex-col border border-border bg-panel shadow-xl">
        <header className="border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold text-text">
            Importar desde el explorador
          </h2>
          <p className="mt-0.5 text-[12px] text-muted">
            Destino: carpeta actual · copia por defecto · Shift al soltar preselecciona
            mover
          </p>
        </header>

        <div className="min-h-0 flex-1 overflow-auto px-4 py-3">
          {error && <div className="mb-2 text-[12px] text-error">{error}</div>}
          {!preview && !error && (
            <div className="text-[12px] text-muted">Analizando rutas…</div>
          )}
          {preview && (
            <>
              {preview.engineering_warning && (
                <div className="mb-3 flex gap-2 border border-warn/40 bg-elevated px-3 py-2 text-[12px] text-warn">
                  <AlertTriangle size={14} className="mt-0.5 shrink-0" />
                  <span>{preview.engineering_warning}</span>
                </div>
              )}
              {preview.conflict_count > 0 && (
                <div className="mb-2 text-[12px] text-warn">
                  {preview.conflict_count} conflicto(s) de nombre en destino
                </div>
              )}
              <ul className="space-y-1.5">
                {preview.items.map((item) => (
                  <li
                    key={item.source_path}
                    className="flex items-start gap-2 border-b border-border/40 py-1.5 text-[12.5px]"
                  >
                    {item.is_directory ? (
                      <Folder size={14} className="mt-0.5 shrink-0 text-accent" />
                    ) : (
                      <File size={14} className="mt-0.5 shrink-0 text-muted" />
                    )}
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-text">{item.file_name}</div>
                      <div className="truncate text-[11px] text-muted">
                        {item.source_path}
                      </div>
                      {item.conflict && (
                        <div className="text-[11px] text-warn">
                          Conflicto → se usará «{item.planned_name}»
                        </div>
                      )}
                    </div>
                    <div className="shrink-0 tabular-nums text-muted">
                      {item.is_directory
                        ? "carpeta"
                        : formatBytes(item.size_bytes)}
                      {item.is_engineering ? " · eng" : ""}
                    </div>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-3 border-t border-border px-4 py-3">
          <label className="flex items-center gap-1.5 text-[12px] text-muted">
            Modo
            <select
              className="h-[28px] rounded-[4px] border border-border bg-elevated px-2 text-text"
              value={mode}
              disabled={busy}
              onChange={(e) => setMode(e.target.value as ExternalDropMode)}
            >
              <option value="copy">Copiar (default)</option>
              <option value="move">Mover</option>
            </select>
          </label>
          <label className="flex items-center gap-1.5 text-[12px] text-muted">
            Conflictos
            <select
              className="h-[28px] rounded-[4px] border border-border bg-elevated px-2 text-text"
              value={conflictPolicy}
              disabled={busy}
              onChange={(e) =>
                setConflictPolicy(e.target.value as ConflictPolicy)
              }
            >
              <option value="rename">Renombrar</option>
              <option value="overwrite">Sobrescribir</option>
              <option value="abort">Abortar si hay conflicto</option>
            </select>
          </label>
          <div className="ml-auto flex gap-2">
            <Button variant="ghost" size="sm" disabled={busy} onClick={onClose}>
              Cancelar
            </Button>
            <Button
              size="sm"
              disabled={busy || !preview || !!error}
              onClick={() => void confirm()}
            >
              {busy ? "Importando…" : "Confirmar"}
            </Button>
          </div>
        </div>

        {progress && !progress.done && (
          <div className="border-t border-border bg-elevated px-4 py-2 text-[11px] text-muted">
            {progress.current}/{progress.total}
            {progress.current_path
              ? ` · ${progress.current_path}`
              : progress.message
                ? ` · ${progress.message}`
                : ""}
          </div>
        )}
      </div>
    </div>
  );
}
