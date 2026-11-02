import { useEffect, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "./ui/button";
import { useUiStore } from "@/stores/ui-store";

interface Props {
  open: boolean;
  onClose: () => void;
}

export function CreateProjectDialog({ open, onClose }: Props) {
  const qc = useQueryClient();
  const setView = useUiStore((s) => s.setView);
  const [name, setName] = useState("");
  const [clientName, setClientName] = useState("");
  const [description, setDescription] = useState("");
  const [startDate, setStartDate] = useState("");
  const [error, setError] = useState<string | null>(null);

  const codeQuery = useQuery({
    queryKey: ["next-code"],
    queryFn: () => api.previewNextProjectCode(),
    enabled: open,
  });

  useEffect(() => {
    if (!open) {
      setName("");
      setClientName("");
      setDescription("");
      setStartDate("");
      setError(null);
    }
  }, [open]);

  const mutation = useMutation({
    mutationFn: () =>
      api.createProject({
        name,
        client_name: clientName || null,
        description: description || null,
        start_date: startDate || null,
      }),
    onSuccess: async (project) => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["recent-projects"] });
      await qc.invalidateQueries({ queryKey: ["app-status"] });
      onClose();
      await api.openProject(project.id);
      setView({ kind: "project", projectId: project.id });
      try {
        await api.reindexRoot();
      } catch {
        /* optional */
      }
    },
    onError: (e: Error) => setError(String(e)),
  });

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55">
      <div className="w-[480px] border border-border bg-panel shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold">Nuevo proyecto</h2>
          <p className="text-[12px] text-muted">
            Código automático AANN · carpetas estándar en disco
          </p>
        </div>

        <div className="space-y-3 p-4">
          <Field label="Código">
            <div className="h-[34px] border border-border bg-elevated px-2.5 py-1.5 font-mono text-[13px] text-accent">
              {codeQuery.data ?? "····"}
            </div>
          </Field>
          <Field label="Nombre *">
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Edificio Torre Norte"
              autoFocus
            />
          </Field>
          <Field label="Cliente">
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={clientName}
              onChange={(e) => setClientName(e.target.value)}
              placeholder="Opcional"
            />
          </Field>
          <Field label="Descripción">
            <textarea
              className="min-h-[72px] w-full resize-y border border-border bg-elevated px-2.5 py-2"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </Field>
          <Field label="Fecha inicio">
            <input
              type="date"
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={startDate}
              onChange={(e) => setStartDate(e.target.value)}
            />
          </Field>
          {error && (
            <div className="border border-error/40 bg-error/10 px-3 py-2 text-[12.5px] text-error">
              {error}
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" onClick={onClose} disabled={mutation.isPending}>
            Cancelar
          </Button>
          <Button
            disabled={!name.trim() || mutation.isPending}
            onClick={() => mutation.mutate()}
          >
            {mutation.isPending ? "Creando…" : "Crear proyecto"}
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
