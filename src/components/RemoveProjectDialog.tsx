import { useEffect, useState, type ReactNode } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import type { Project } from "@/lib/types";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";

interface Props {
  project: Project;
  onClose: () => void;
}

export function RemoveProjectDialog({ project, onClose }: Props) {
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
  }, [project.id]);

  const mutation = useMutation({
    mutationFn: () => api.removeProjectRegistration(project.id, typed),
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

  const canConfirm = typed.trim() === expected;

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/55">
      <div className="w-[480px] border border-border bg-panel shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold">Quitar de IngManager</h2>
          <p className="mt-1 text-[12px] text-muted">
            {project.code ?? "—"} · {project.name}
          </p>
        </div>
        <div className="space-y-3 p-4 text-[12.5px]">
          <p>
            Se elimina solo el registro en la base de datos. La carpeta{" "}
            <span className="font-mono">{project.root_relative_path}</span>{" "}
            permanece intacta en disco. No se abre el proyecto ni se indexa.
          </p>
          <Field label={`Escriba «${expected}» para confirmar`}>
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5 font-mono"
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              autoFocus
            />
          </Field>
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
            disabled={!canConfirm || mutation.isPending}
            onClick={() => mutation.mutate()}
          >
            {mutation.isPending ? "Quitando…" : "Quitar registro"}
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
