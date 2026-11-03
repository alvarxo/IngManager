import { useEffect, useState, type ReactNode } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import type { Project } from "@/lib/types";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";

type Mode = "remove" | "delete";

interface Props {
  project: Project;
  mode: Mode;
  onClose: () => void;
}

export function DeleteProjectDialog({ project, mode, onClose }: Props) {
  const qc = useQueryClient();
  const view = useUiStore((s) => s.view);
  const setView = useUiStore((s) => s.setView);
  const expected =
    project.code?.trim() || project.root_relative_path.trim() || project.name;
  const [typed, setTyped] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setTyped("");
    setError(null);
  }, [project.id, mode]);

  const mutation = useMutation({
    mutationFn: async () => {
      if (mode === "remove") {
        return api.removeProjectFromApp(project.id);
      }
      return api.deleteProjectWithFiles(project.id, typed);
    },
    onSuccess: async (result) => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["recent-projects"] });
      await qc.invalidateQueries({ queryKey: ["favorite-projects"] });
      await qc.invalidateQueries({ queryKey: ["app-status"] });
      if (view.kind === "project" && view.projectId === project.id) {
        setView({ kind: "home" });
      }
      onClose();
      window.alert(result.message);
    },
    onError: (e: Error) => setError(String(e).replace(/^Error:\s*/, "")),
  });

  const canDelete =
    mode === "remove" || typed.trim() === expected;

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/55">
      <div className="w-[480px] border border-border bg-panel shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold">
            {mode === "remove"
              ? "Quitar de IngManager"
              : "Eliminar proyecto y archivos"}
          </h2>
          <p className="mt-1 text-[12px] text-muted">
            {project.code ?? "—"} · {project.name}
          </p>
        </div>
        <div className="space-y-3 p-4 text-[12.5px]">
          {mode === "remove" ? (
            <p>
              Se elimina solo el registro en la base de datos. La carpeta{" "}
              <span className="font-mono">{project.root_relative_path}</span>{" "}
              permanece en disco.
            </p>
          ) : project.import_mode === "linked" ? (
            <p>
              Proyecto <strong>vinculado</strong>: solo se quita el registro.
              Los archivos no se tocan.
            </p>
          ) : (
            <p>
              La carpeta del proyecto se enviará a la{" "}
              <strong>papelera del sistema</strong> (no a la papelera interna
              de IngManager). Esta acción requiere confirmación explícita.
            </p>
          )}
          {mode === "delete" && (
            <Field label={`Escriba «${expected}» para confirmar`}>
              <input
                className="h-[34px] w-full border border-border bg-elevated px-2.5 font-mono"
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
                autoFocus
              />
            </Field>
          )}
          {error && (
            <div className="border border-error/40 bg-error/10 px-3 py-2 text-error">
              {error}
            </div>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button
            variant="secondary"
            onClick={onClose}
            disabled={mutation.isPending}
          >
            Cancelar
          </Button>
          <Button
            disabled={!canDelete || mutation.isPending}
            onClick={() => mutation.mutate()}
          >
            {mutation.isPending
              ? "Procesando…"
              : mode === "remove"
                ? "Quitar registro"
                : "Eliminar"}
          </Button>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="block">
      <div className="mb-1 text-[11.5px] font-medium text-muted">{label}</div>
      {children}
    </label>
  );
}
