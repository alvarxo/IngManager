import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import type { DetectedFolder, ImportMode, ImportPreview } from "@/lib/types";
import { Button } from "./ui/button";

interface Props {
  onDone?: (projectId: string) => void;
}

export function ImportWizard({ onDone }: Props) {
  const qc = useQueryClient();
  const [step, setStep] = useState<"pick" | "detect" | "preview" | "done">(
    "pick",
  );
  const [sourceRoot, setSourceRoot] = useState<string | null>(null);
  const [candidates, setCandidates] = useState<DetectedFolder[]>([]);
  const [selected, setSelected] = useState<DetectedFolder | null>(null);
  const [mode, setMode] = useState<ImportMode>("link");
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [folderName, setFolderName] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [confirmOverwrite, setConfirmOverwrite] = useState(false);
  const [authorizeParent, setAuthorizeParent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [resultMsg, setResultMsg] = useState<string | null>(null);

  async function pickSource() {
    setError(null);
    const selectedPath = await open({
      directory: true,
      multiple: false,
      title: "Carpeta con proyecto(s) existente(s)",
    });
    if (typeof selectedPath !== "string") return;
    setSourceRoot(selectedPath);
    try {
      const found = await api.detectImportableProjects(selectedPath);
      setCandidates(found);
      setStep("detect");
      if (found.length === 1) {
        await chooseCandidate(found[0]!);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function chooseCandidate(folder: DetectedFolder) {
    setError(null);
    setSelected(folder);
    setName(folder.suggested_name);
    setFolderName(folder.name);
    const defaultMode: ImportMode = folder.under_authorized_root
      ? "link"
      : "copy";
    setMode(defaultMode);
    try {
      const proposed = await api.proposeImportCode(folder.suggested_code);
      setCode(proposed ?? folder.suggested_code ?? "");
      const prev = await api.previewImportProject(
        folder.absolute_path,
        proposed,
        defaultMode,
        folder.name,
        folder.authorized_root_id,
      );
      setPreview(prev);
      setConfirmOverwrite(false);
      setAuthorizeParent(false);
      setStep("preview");
    } catch (e) {
      setError(String(e));
    }
  }

  async function refreshPreview(nextMode = mode) {
    if (!selected) return;
    setError(null);
    try {
      const prev = await api.previewImportProject(
        selected.absolute_path,
        code || null,
        nextMode,
        folderName || null,
        selected.authorized_root_id,
      );
      setPreview(prev);
    } catch (e) {
      setError(String(e));
    }
  }

  const importMut = useMutation({
    mutationFn: () =>
      api.importExistingProject({
        source_path: selected!.absolute_path,
        code: code.trim() || null,
        name,
        mode,
        folder_name: mode === "copy" ? folderName || null : null,
        authorized_root_id: selected!.authorized_root_id,
        confirm_overwrite: confirmOverwrite,
        authorize_parent_path:
          authorizeParent && preview?.suggested_authorize_path
            ? preview.suggested_authorize_path
            : null,
        create_missing_folders: mode === "copy",
      }),
    onSuccess: async (res) => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["app-status"] });
      await qc.invalidateQueries({ queryKey: ["recent-projects"] });
      await qc.invalidateQueries({ queryKey: ["authorized-roots"] });
      const label = res.code ?? res.target_path;
      setResultMsg(
        res.linked
          ? `Vinculado ${label} en su ubicación (sin copia).`
          : `Copiado ${label} → ${res.target_path}.`,
      );
      setStep("done");
      onDone?.(res.project_id);
      try {
        await api.reindexRoot();
      } catch {
        /* optional */
      }
    },
    onError: (e: Error) => setError(String(e)),
  });

  const canImport =
    !!name.trim() &&
    !!preview &&
    !(preview.will_overwrite && !confirmOverwrite) &&
    !(preview.requires_authorize_root && !authorizeParent && mode !== "copy");

  return (
    <div className="space-y-3">
      {step === "pick" && (
        <>
          <p className="text-[12.5px] text-muted">
            Por defecto se vincula la carpeta en su sitio (sin copiar, mover ni
            renombrar). También puede copiar o dejar la reorganización para
            después. Sin sobrescritura silenciosa.
          </p>
          <Button onClick={() => void pickSource()}>Elegir carpeta fuente…</Button>
        </>
      )}

      {step === "detect" && (
        <>
          <p className="text-[12px] text-muted">
            Fuente: <span className="font-mono">{sourceRoot}</span>
          </p>
          {(candidates.length ?? 0) === 0 ? (
            <p className="text-[12.5px] text-muted">
              No se detectaron carpetas de proyecto. Vuelva a elegir.
            </p>
          ) : (
            <ul className="max-h-48 space-y-1 overflow-auto">
              {candidates.map((c) => (
                <li key={c.absolute_path}>
                  <button
                    type="button"
                    className="flex w-full flex-col items-start border border-border/60 px-2 py-1.5 text-left text-[12.5px] hover:bg-elevated"
                    onClick={() => void chooseCandidate(c)}
                  >
                    <span className="font-medium">{c.name}</span>
                    <span className="text-[11px] text-muted">
                      {c.suggested_code
                        ? `código sugerido ${c.suggested_code}`
                        : "sin código"}
                      {c.under_authorized_root ? " · bajo raíz" : " · fuera de raíz"}
                      {c.conflict_with_existing ? " · conflicto" : ""}
                      {" · "}
                      {c.matched_standard_folders.length} carpetas mapeadas
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <Button variant="secondary" onClick={() => setStep("pick")}>
            Volver
          </Button>
        </>
      )}

      {step === "preview" && selected && preview && (
        <>
          <div className="flex flex-wrap gap-2 text-[12px]">
            {(
              [
                ["link", "Vincular in situ"],
                ["copy", "Copiar"],
                ["reorganize_later", "Reorganizar después"],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                type="button"
                className={
                  mode === value
                    ? "border border-accent bg-selection px-2 py-1"
                    : "border border-border px-2 py-1 hover:bg-elevated"
                }
                onClick={() => {
                  setMode(value);
                  void refreshPreview(value);
                }}
              >
                {label}
              </button>
            ))}
          </div>
          <label className="block text-[12px]">
            <span className="mb-1 block text-muted">Código (opcional)</span>
            <input
              className="h-[32px] w-full border border-border bg-elevated px-2 font-mono"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              onBlur={() => void refreshPreview()}
            />
          </label>
          <label className="block text-[12px]">
            <span className="mb-1 block text-muted">Nombre *</span>
            <input
              className="h-[32px] w-full border border-border bg-elevated px-2"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          {mode === "copy" && (
            <label className="block text-[12px]">
              <span className="mb-1 block text-muted">
                Carpeta destino (independiente del código)
              </span>
              <input
                className="h-[32px] w-full border border-border bg-elevated px-2 font-mono"
                value={folderName}
                onChange={(e) => setFolderName(e.target.value)}
                onBlur={() => void refreshPreview()}
              />
            </label>
          )}
          <div className="border border-border bg-elevated px-3 py-2 text-[12px]">
            <div>
              Destino:{" "}
              <span className="font-mono">{preview.target_absolute_path}</span>
            </div>
            <div>
              {preview.link_in_place
                ? "Sin copia — registro sobre la carpeta existente"
                : `~${preview.file_count_hint} archivos a copiar`}
            </div>
            {preview.category_mappings.filter((m) => m.matched).length > 0 && (
              <div className="mt-1 text-muted">
                Categorías mapeadas:{" "}
                {preview.category_mappings
                  .filter((m) => m.matched)
                  .map((m) => `${m.name}→${m.relative_path}`)
                  .join(", ")}
              </div>
            )}
            {preview.requires_authorize_root && (
              <div className="mt-1 text-warn">
                La carpeta no está bajo una raíz autorizada. Autorice el padre
                o use modo copia.
              </div>
            )}
            {preview.will_overwrite && (
              <div className="mt-1 text-warn">
                La carpeta destino ya existe. Confirme explícitamente para
                mezclar (sin sobrescritura silenciosa).
              </div>
            )}
          </div>
          {preview.requires_authorize_root && mode !== "copy" && (
            <label className="flex items-center gap-2 text-[12.5px]">
              <input
                type="checkbox"
                checked={authorizeParent}
                onChange={(e) => setAuthorizeParent(e.target.checked)}
              />
              Autorizar padre:{" "}
              <span className="font-mono text-[11px]">
                {preview.suggested_authorize_path}
              </span>
            </label>
          )}
          {preview.will_overwrite && (
            <label className="flex items-center gap-2 text-[12.5px]">
              <input
                type="checkbox"
                checked={confirmOverwrite}
                onChange={(e) => setConfirmOverwrite(e.target.checked)}
              />
              Confirmo mezcla / sobrescritura selectiva
            </label>
          )}
          <div className="flex gap-2">
            <Button
              onClick={() => importMut.mutate()}
              disabled={importMut.isPending || !canImport}
            >
              {importMut.isPending
                ? "Importando…"
                : mode === "copy"
                  ? "Importar por copia"
                  : mode === "reorganize_later"
                    ? "Vincular (reorganizar después)"
                    : "Vincular in situ"}
            </Button>
            <Button variant="secondary" onClick={() => setStep("detect")}>
              Volver
            </Button>
          </div>
        </>
      )}

      {step === "done" && (
        <>
          <p className="text-[13px] text-accent">{resultMsg}</p>
          <Button
            variant="secondary"
            onClick={() => {
              setStep("pick");
              setSelected(null);
              setPreview(null);
              setResultMsg(null);
            }}
          >
            Importar otro
          </Button>
        </>
      )}

      {error && (
        <div className="border border-error/40 bg-error/10 px-2 py-1.5 text-[12px] text-error">
          {error}
        </div>
      )}
    </div>
  );
}
