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
  const [code, setCode] = useState("");
  const [folderName, setFolderName] = useState("");
  const [useSuggestedCode, setUseSuggestedCode] = useState(true);
  const [rootId, setRootId] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [folderConflict, setFolderConflict] = useState<{
    folderName: string;
  } | null>(null);

  const codeQuery = useQuery({
    queryKey: ["next-code"],
    queryFn: () => api.previewNextProjectCode(),
    enabled: open,
  });

  const rootsQuery = useQuery({
    queryKey: ["authorized-roots"],
    queryFn: () => api.listAuthorizedRoots(),
    enabled: open,
  });

  useEffect(() => {
    if (!open) {
      setName("");
      setClientName("");
      setDescription("");
      setStartDate("");
      setCode("");
      setFolderName("");
      setUseSuggestedCode(true);
      setRootId("");
      setError(null);
      setFolderConflict(null);
    }
  }, [open]);

  useEffect(() => {
    if (useSuggestedCode && codeQuery.data) {
      setCode(codeQuery.data);
    }
  }, [useSuggestedCode, codeQuery.data]);

  useEffect(() => {
    if (!rootId && rootsQuery.data?.length) {
      const primary =
        rootsQuery.data.find((r) => r.is_primary === 1) ?? rootsQuery.data[0];
      if (primary) setRootId(primary.id);
    }
  }, [rootsQuery.data, rootId]);

  const mutation = useMutation({
    mutationFn: (policy: string = "abort") =>
      api.createProject({
        name,
        client_name: clientName || null,
        description: description || null,
        start_date: startDate || null,
        code: code.trim() || null,
        folder_name: folderName.trim() || null,
        authorized_root_id: rootId || null,
        create_standard_folders: true,
        folder_conflict_policy: policy,
      }),
    onSuccess: async (project) => {
      setFolderConflict(null);
      await qc.invalidateQueries({ queryKey: ["projects"] });
      await qc.invalidateQueries({ queryKey: ["recent-projects"] });
      await qc.invalidateQueries({ queryKey: ["app-status"] });
      onClose();
      setView({ kind: "project", projectId: project.id });
      try {
        await api.openProject(project.id);
      } catch {
        /* open errors are non-fatal after create */
      }
      try {
        await api.reindexRoot();
      } catch {
        /* optional */
      }
    },
    onError: (e: Error) => {
      const msg = String(e).replace(/^Error:\s*/, "");
      if (msg.startsWith("FOLDER_EXISTS:")) {
        const parts = msg.split(":");
        setFolderConflict({ folderName: parts[1] ?? "" });
        setError(parts.slice(2).join(":") || msg);
        return;
      }
      setFolderConflict(null);
      setError(msg);
    },
  });

  if (!open) return null;

  const suggestedFolder =
    folderName.trim() ||
    code.trim() ||
    name.trim().replace(/\s+/g, "_") ||
    "…";

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55">
      <div className="w-[520px] border border-border bg-panel shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold">Nuevo proyecto</h2>
          <p className="text-[12px] text-muted">
            UUID interno · código opcional · carpeta física independiente
          </p>
        </div>

        <div className="max-h-[70vh] space-y-3 overflow-auto p-4">
          <Field label="Nombre *">
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Edificio Torre Norte"
              autoFocus
            />
          </Field>
          <Field label="Código de negocio (opcional)">
            <div className="flex gap-2">
              <input
                className="h-[34px] min-w-0 flex-1 border border-border bg-elevated px-2.5 font-mono"
                value={code}
                onChange={(e) => {
                  setUseSuggestedCode(false);
                  setCode(e.target.value);
                }}
                placeholder="AANN o personalizado"
              />
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => {
                  setUseSuggestedCode(true);
                  if (codeQuery.data) setCode(codeQuery.data);
                }}
              >
                Sugerir
              </Button>
            </div>
          </Field>
          <Field label="Carpeta física (opcional)">
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5 font-mono"
              value={folderName}
              onChange={(e) => setFolderName(e.target.value)}
              placeholder={`Por defecto: ${suggestedFolder}`}
            />
            <p className="mt-1 text-[11px] text-muted">
              Independiente del código. No se renombra al cambiar el código.
            </p>
          </Field>
          <Field label="Raíz autorizada">
            <select
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={rootId}
              onChange={(e) => setRootId(e.target.value)}
            >
              {(rootsQuery.data ?? []).map((r) => (
                <option key={r.id} value={r.id}>
                  {r.label || r.absolute_path}
                  {r.is_primary === 1 ? " (principal)" : ""}
                </option>
              ))}
            </select>
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
          {folderConflict && (
            <div className="space-y-2 border border-warn/40 bg-warn/10 px-3 py-2 text-[12.5px]">
              <p>
                Conflicto con la carpeta física{" "}
                <span className="font-mono">{folderConflict.folderName}</span>.
                Elija cómo recrear el proyecto (p. ej. código 1090):
              </p>
              <div className="flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={mutation.isPending}
                  onClick={() => mutation.mutate("use_existing")}
                >
                  Usar carpeta existente
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={mutation.isPending}
                  onClick={() => mutation.mutate("rename")}
                >
                  Crear con otro nombre
                </Button>
              </div>
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" onClick={onClose} disabled={mutation.isPending}>
            Cancelar
          </Button>
          <Button
            disabled={!name.trim() || mutation.isPending}
            onClick={() => mutation.mutate("abort")}
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
