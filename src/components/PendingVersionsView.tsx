import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { GitBranch, Check, X } from "lucide-react";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";

export function PendingVersionsView() {
  const qc = useQueryClient();
  const [message, setMessage] = useState<string | null>(null);
  const [resolveDoc, setResolveDoc] = useState<Record<string, string>>({});

  const pending = useQuery({
    queryKey: ["pending-versions"],
    queryFn: () => api.listPendingVersions(null),
  });

  const docsByProject = useQuery({
    queryKey: ["projects"],
    queryFn: () => api.listProjects(false),
  });

  const resolve = useMutation({
    mutationFn: ({
      pendingId,
      documentId,
    }: {
      pendingId: string;
      documentId: string;
    }) => api.resolvePendingVersion(pendingId, documentId),
    onSuccess: (r) => {
      void qc.invalidateQueries({ queryKey: ["pending-versions"] });
      setMessage(r.message);
    },
    onError: (e) => setMessage(String(e)),
  });

  const dismiss = useMutation({
    mutationFn: (id: string) => api.dismissPendingVersion(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["pending-versions"] });
    },
  });

  const rows = pending.data ?? [];

  return (
    <div className="flex h-full flex-col bg-bg">
      <header className="flex items-center justify-between border-b border-border px-4 py-3">
        <div>
          <h1 className="text-[15px] font-semibold text-text">
            Versiones pendientes
          </h1>
          <p className="text-[12px] text-muted">
            Ambiguas o sin documento — resuelva manualmente. Sobrescrituras no
            generan versión falsa.
          </p>
        </div>
        <GitBranch size={18} className="text-muted" />
      </header>

      {message && (
        <div className="border-b border-border px-4 py-2 text-[12px] text-muted">
          {message}
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-auto p-4">
        <table className="w-full text-left text-[13px]">
          <thead className="text-[11px] uppercase text-muted">
            <tr>
              <th className="py-2">Archivo</th>
              <th className="py-2">Stem</th>
              <th className="py-2">Motivo</th>
              <th className="py-2">Asignar a documento</th>
              <th className="py-2" />
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <PendingRow
                key={row.id}
                row={row}
                documentId={resolveDoc[row.id] ?? ""}
                onDocumentId={(id) =>
                  setResolveDoc((prev) => ({ ...prev, [row.id]: id }))
                }
                projectName={
                  docsByProject.data?.find((p) => p.id === row.project_id)
                    ?.name
                }
                onResolve={() => {
                  const docId = resolveDoc[row.id];
                  if (!docId) {
                    setMessage("Seleccione un documento");
                    return;
                  }
                  resolve.mutate({ pendingId: row.id, documentId: docId });
                }}
                onDismiss={() => dismiss.mutate(row.id)}
              />
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={5} className="py-10 text-center text-muted">
                  No hay eventos pendientes
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function PendingRow({
  row,
  documentId,
  onDocumentId,
  projectName,
  onResolve,
  onDismiss,
}: {
  row: {
    id: string;
    project_id: string;
    file_name: string;
    base_stem: string;
    reason: string;
    absolute_path: string;
    candidate_document_ids_json: string | null;
  };
  documentId: string;
  onDocumentId: (id: string) => void;
  projectName?: string;
  onResolve: () => void;
  onDismiss: () => void;
}) {
  const docs = useQuery({
    queryKey: ["documents", row.project_id],
    queryFn: () => api.listDocuments(row.project_id, null, true),
  });

  let candidates: string[] = [];
  try {
    candidates = row.candidate_document_ids_json
      ? (JSON.parse(row.candidate_document_ids_json) as string[])
      : [];
  } catch {
    candidates = [];
  }

  const options = (docs.data ?? []).filter(
    (d) => candidates.length === 0 || candidates.includes(d.id),
  );

  return (
    <tr className="border-t border-border/60 align-top">
      <td className="py-2">
        <div className="text-text">{row.file_name}</div>
        <div className="text-[11px] text-muted">
          {projectName || row.project_id}
        </div>
        <div className="max-w-[320px] truncate text-[11px] text-muted">
          {row.absolute_path}
        </div>
      </td>
      <td className="py-2 text-muted">{row.base_stem}</td>
      <td className="py-2 text-muted">{row.reason}</td>
      <td className="py-2">
        <select
          className="h-[32px] w-full max-w-[220px] rounded-[var(--radius-sm)] border border-border bg-elevated px-2 text-[12px] text-text"
          value={documentId}
          onChange={(e) => onDocumentId(e.target.value)}
        >
          <option value="">— elegir —</option>
          {(options.length ? options : docs.data ?? []).map((d) => (
            <option key={d.id} value={d.id}>
              {d.business_code ? `${d.business_code} · ` : ""}
              {d.title}
            </option>
          ))}
        </select>
      </td>
      <td className="py-2">
        <div className="flex gap-1">
          <Button type="button" variant="secondary" onClick={onResolve}>
            <Check size={14} />
          </Button>
          <Button type="button" variant="ghost" onClick={onDismiss}>
            <X size={14} />
          </Button>
        </div>
      </td>
    </tr>
  );
}
