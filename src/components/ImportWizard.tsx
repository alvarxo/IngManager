import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import type { DetectedFolder, ImportPreview } from "@/lib/types";
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
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [confirmOverwrite, setConfirmOverwrite] = useState(false);
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
    try {
      const proposed = await api.proposeImportCode(folder.suggested_code);
      setCode(proposed);
      const prev = await api.previewImportProject(
        folder.absolute_path,
        proposed,
      );
      setPreview(prev);
      setConfirmOverwrite(false);
      setStep("preview");
    } catch (e) {
      setError(String(e));
    }
  }

  async function refreshPreview() {
    if (!selected) return;
    setError(null);
    try {
      const prev = await api.previewImportProject(selected.absolute_path, code);
      setPreview(prev);
    } catch (e) {
      setError(String(e));
    }
  }

  const importMut = useMutation({
    mutationFn: () =>
      api.importExistingProject({
        source_path: selected!.absolute_path,
        code,
        name,
        confirm_overwrite: confirmOverwrite,
        copy_files: true,
      }),
    onSuccess: async (res) => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["app-status"] });
      await qc.invalidateQueries({ queryKey: ["recent-projects"] });
      setResultMsg(
        `Importado ${res.code} → ${res.target_path} (copia completada).`,
      );
      setStep("done");
      onDone?.(res.project_id);
    },
    onError: (e: Error) => setError(String(e)),
  });

  return (
    <div className="space-y-3">
      {step === "pick" && (
        <>
          <p className="text-[12.5px] text-muted">
            Importación por copia: detecta carpetas tipo AANN o con estructura
            estándar, propone código, muestra preview y no sobrescribe sin
            confirmación.
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
                        : "sin código AANN"}
                      {c.conflict_with_existing ? " · conflicto" : ""}
                      {" · "}
                      {c.matched_standard_folders.length} carpetas estándar
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
          <label className="block text-[12px]">
            <span className="mb-1 block text-muted">Código (AANN)</span>
            <input
              className="h-[32px] w-full border border-border bg-elevated px-2 font-mono"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              onBlur={() => void refreshPreview()}
            />
          </label>
          <label className="block text-[12px]">
            <span className="mb-1 block text-muted">Nombre</span>
            <input
              className="h-[32px] w-full border border-border bg-elevated px-2"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <div className="border border-border bg-elevated px-3 py-2 text-[12px]">
            <div>
              Destino:{" "}
              <span className="font-mono">{preview.target_absolute_path}</span>
            </div>
            <div>~{preview.file_count_hint} archivos a copiar</div>
            {preview.will_overwrite && (
              <div className="mt-1 text-warn">
                La carpeta destino ya existe. Confirme explícitamente para
                mezclar (sin renombrado masivo; archivos existentes se omiten
                salvo confirmación).
              </div>
            )}
          </div>
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
              disabled={
                importMut.isPending ||
                !code.trim() ||
                !name.trim() ||
                (preview.will_overwrite && !confirmOverwrite)
              }
            >
              {importMut.isPending ? "Importando…" : "Importar por copia"}
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
