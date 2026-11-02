import { useMemo, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Link2, PackagePlus, Send, Inbox } from "lucide-react";
import { api } from "@/lib/api";
import type { PreviewSentResult, SendItemInput } from "@/lib/types";
import { cn } from "@/lib/utils";
import { isoToUiDate, todayIso, todayUi, uiDateToIso } from "@/lib/dates";
import { useUiStore } from "@/stores/ui-store";
import { Button } from "./ui/button";

interface Props {
  projectId: string;
}

export function TransmissionsPanel({ projectId }: Props) {
  const qc = useQueryClient();
  const selectedId = useUiStore((s) => s.selectedTransmissionId);
  const setSelectedId = useUiStore((s) => s.setSelectedTransmissionId);

  const [filter, setFilter] = useState<"all" | "sent" | "received">("all");
  const [sendOpen, setSendOpen] = useState(false);
  const [recvOpen, setRecvOpen] = useState(false);

  const list = useQuery({
    queryKey: ["transmissions", projectId, filter],
    queryFn: () =>
      api.listTransmissions(projectId, filter === "all" ? null : filter),
  });

  const detail = useQuery({
    queryKey: ["transmission", selectedId],
    queryFn: () => api.getTransmission(selectedId!),
    enabled: !!selectedId,
  });

  const items = useQuery({
    queryKey: ["transmission-items", selectedId],
    queryFn: () => api.listTransmissionItems(selectedId!),
    enabled: !!selectedId,
  });

  const links = useQuery({
    queryKey: ["transmission-links", selectedId],
    queryFn: () => api.listTransmissionLinks(selectedId!),
    enabled: !!selectedId,
  });

  const sentOptions = useMemo(
    () => list.data?.filter((t) => t.direction === "sent") ?? [],
    [list.data],
  );

  return (
    <div className="flex h-full min-w-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col border-r border-border">
        <div className="flex flex-wrap items-center gap-2 border-b border-border bg-panel px-3 py-1.5">
          <Button size="sm" onClick={() => setSendOpen(true)}>
            <Send size={14} /> Nuevo envío (ENV)
          </Button>
          <Button size="sm" variant="secondary" onClick={() => setRecvOpen(true)}>
            <Inbox size={14} /> Registrar recibido (REC)
          </Button>
          <div className="ml-auto flex gap-1">
            {(["all", "sent", "received"] as const).map((f) => (
              <Button
                key={f}
                size="sm"
                variant={filter === f ? "default" : "ghost"}
                onClick={() => setFilter(f)}
              >
                {f === "all" ? "Todas" : f === "sent" ? "Enviadas" : "Recibidas"}
              </Button>
            ))}
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto">
          {list.isLoading && <div className="p-4 text-muted">Cargando…</div>}
          {list.error && (
            <div className="p-4 text-error">{String(list.error)}</div>
          )}
          {!list.isLoading && (list.data?.length ?? 0) === 0 && (
            <div className="p-8 text-center text-muted">
              Sin transmisiones. Cree un envío ENV o importe un REC.
            </div>
          )}
          <table className="w-full border-collapse text-left">
            <thead className="sticky top-0 bg-panel">
              <tr className="border-b border-border text-[11px] uppercase text-muted">
                <th className="h-[36px] px-3 font-medium">Código</th>
                <th className="w-[70px] px-3 font-medium">Dir</th>
                <th className="px-3 font-medium">Asunto</th>
                <th className="px-3 font-medium">Contraparte</th>
                <th className="w-[100px] px-3 font-medium">Fecha</th>
                <th className="w-[70px] px-3 font-medium">Ítems</th>
                <th className="w-[80px] px-3 font-medium">Estado</th>
              </tr>
            </thead>
            <tbody>
              {list.data?.map((t) => (
                <tr
                  key={t.id}
                  className={cn(
                    "h-[40px] cursor-pointer border-b border-border/50",
                    selectedId === t.id ? "bg-selection" : "hover:bg-elevated",
                  )}
                  onClick={() => setSelectedId(t.id)}
                >
                  <td className="px-3 font-mono text-[12px] text-accent">
                    {t.code ?? "—"}
                  </td>
                  <td className="px-3 text-[11px] uppercase text-muted">
                    {t.direction === "sent" ? "ENV" : "REC"}
                  </td>
                  <td className="truncate px-3">{t.subject ?? "—"}</td>
                  <td className="truncate px-3 text-muted">
                    {t.counterpart ?? "—"}
                  </td>
                  <td className="px-3 text-[12px]">{isoToUiDate(t.sent_at)}</td>
                  <td className="px-3 tabular-nums text-muted">{t.item_count}</td>
                  <td className="px-3 text-[11px] text-muted">{t.status}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <aside className="flex w-[360px] shrink-0 flex-col bg-panel">
        <div className="border-b border-border px-3 py-2 text-[11px] font-semibold uppercase text-muted">
          Detalle transmisión
        </div>
        <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3 text-[12.5px]">
          {!selectedId && (
            <p className="text-muted">Seleccione una transmisión del listado.</p>
          )}
          {detail.data && (
            <>
              <div className="font-mono text-[14px] text-accent">
                {detail.data.code}
              </div>
              <div className="text-[14px] font-medium">{detail.data.subject}</div>
              <Meta
                label="Dirección"
                value={
                  detail.data.direction === "sent"
                    ? "Enviado (ENV)"
                    : "Recibido (REC)"
                }
              />
              <Meta label="Contraparte" value={detail.data.counterpart ?? "—"} />
              <Meta label="Fecha" value={isoToUiDate(detail.data.sent_at)} />
              <Meta label="Medio" value={detail.data.medium ?? "—"} />
              <Meta label="Estado" value={detail.data.status} />
              <Meta
                label="Carpeta"
                value={detail.data.folder_relative_path ?? "—"}
                mono
              />
              <Meta
                label="ZIP"
                value={detail.data.zip_relative_path ?? "—"}
                mono
              />
              <Meta
                label="Checksum ZIP"
                value={detail.data.zip_checksum ?? "—"}
                mono
              />
              {detail.data.antecedentes_relative_path && (
                <Meta
                  label="Extraído a Antecedentes"
                  value={detail.data.antecedentes_relative_path}
                  mono
                />
              )}
              <Button
                size="sm"
                variant="secondary"
                onClick={() => api.openTransmissionFolder(detail.data!.id)}
              >
                <FolderOpen size={14} /> Abrir carpeta
              </Button>

              <div className="pt-2 text-[11px] font-semibold uppercase text-muted">
                Ítems ({items.data?.length ?? 0})
              </div>
              <ul className="space-y-1">
                {items.data?.map((it) => (
                  <li
                    key={it.id}
                    className="border border-border/60 px-2 py-1 text-[12px]"
                  >
                    <div>{it.file_name}</div>
                    <div className="truncate font-mono text-[10px] text-muted">
                      {it.checksum ? `${it.checksum.slice(0, 16)}…` : "—"}
                    </div>
                  </li>
                ))}
              </ul>

              <div className="pt-2 text-[11px] font-semibold uppercase text-muted">
                Vínculos ENV ↔ REC
              </div>
              {(links.data?.length ?? 0) === 0 && (
                <p className="text-[12px] text-muted">Sin vínculos.</p>
              )}
              <ul className="space-y-1">
                {links.data?.map((l) => (
                  <li key={l.id} className="text-[12px]">
                    <Link2 size={12} className="mr-1 inline" />
                    {l.sent_transmission_id.slice(0, 8)}… ↔{" "}
                    {l.received_transmission_id.slice(0, 8)}…
                  </li>
                ))}
              </ul>

              {detail.data.direction === "received" && (
                <LinkForm
                  sentOptions={sentOptions.map((s) => ({
                    id: s.id,
                    label: `${s.code} — ${s.subject}`,
                  }))}
                  receivedId={detail.data.id}
                  onDone={async () => {
                    await qc.invalidateQueries({
                      queryKey: ["transmission-links", selectedId],
                    });
                    await qc.invalidateQueries({
                      queryKey: ["transmission", selectedId],
                    });
                  }}
                />
              )}
            </>
          )}
        </div>
      </aside>

      {sendOpen && (
        <SendDialog
          projectId={projectId}
          onClose={() => setSendOpen(false)}
          onCreated={async (id) => {
            setSendOpen(false);
            await qc.invalidateQueries({ queryKey: ["transmissions", projectId] });
            setSelectedId(id);
          }}
        />
      )}
      {recvOpen && (
        <ReceiveDialog
          projectId={projectId}
          sentOptions={sentOptions.map((s) => ({
            id: s.id,
            label: `${s.code} — ${s.subject}`,
          }))}
          onClose={() => setRecvOpen(false)}
          onCreated={async (id) => {
            setRecvOpen(false);
            await qc.invalidateQueries({ queryKey: ["transmissions", projectId] });
            setSelectedId(id);
          }}
        />
      )}
    </div>
  );
}

function Meta({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div>
      <div className="text-[10.5px] font-medium uppercase text-muted">{label}</div>
      <div className={mono ? "break-all font-mono text-[11px]" : ""}>{value}</div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <div className="mb-1 text-[11.5px] text-muted">{label}</div>
      {children}
    </label>
  );
}

function LinkForm({
  sentOptions,
  receivedId,
  onDone,
}: {
  sentOptions: { id: string; label: string }[];
  receivedId: string;
  onDone: () => Promise<void>;
}) {
  const [sentId, setSentId] = useState("");
  return (
    <div className="space-y-2 border-t border-border pt-3">
      <div className="text-[11px] text-muted">Vincular a envío ENV</div>
      <select
        className="h-[32px] w-full border border-border bg-elevated px-2"
        value={sentId}
        onChange={(e) => setSentId(e.target.value)}
      >
        <option value="">Seleccionar…</option>
        {sentOptions.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
      <Button
        size="sm"
        disabled={!sentId}
        onClick={async () => {
          await api.linkTransmissions(sentId, receivedId, "Vínculo manual");
          await onDone();
        }}
      >
        <Link2 size={14} /> Vincular
      </Button>
    </div>
  );
}

function SendDialog({
  projectId,
  onClose,
  onCreated,
}: {
  projectId: string;
  onClose: () => void;
  onCreated: (id: string) => Promise<void>;
}) {
  const [counterpart, setCounterpart] = useState("");
  const [subject, setSubject] = useState("");
  const [medium, setMedium] = useState("email");
  const [dateUi, setDateUi] = useState(todayUi());
  const [notes, setNotes] = useState("");
  const [items, setItems] = useState<SendItemInput[]>([]);
  const [preview, setPreview] = useState<PreviewSentResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const codeQ = useQuery({
    queryKey: ["tx-code-env", projectId],
    queryFn: () => api.previewNextTransmissionCode(projectId, "ENV"),
  });

  const previewMut = useMutation({
    mutationFn: () =>
      api.previewSentTransmission({
        project_id: projectId,
        counterpart,
        subject,
        medium,
        notes,
        sent_at: uiDateToIso(dateUi) ?? todayIso(),
        items,
      }),
    onSuccess: (r) => {
      setPreview(r);
      setError(null);
    },
    onError: (e: Error) => setError(String(e)),
  });

  const createMut = useMutation({
    mutationFn: () =>
      api.createSentTransmission({
        project_id: projectId,
        counterpart,
        subject,
        medium,
        notes,
        sent_at: uiDateToIso(dateUi) ?? todayIso(),
        items,
      }),
    onSuccess: (tx) => onCreated(tx.id),
    onError: (e: Error) => setError(String(e)),
  });

  async function addFiles() {
    const picked = await open({
      multiple: true,
      title: "Archivos a incluir en el envío (bajo raíz)",
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    setItems((prev) => [
      ...prev,
      ...paths.map((absolute_path) => ({ absolute_path })),
    ]);
    setPreview(null);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55">
      <div className="max-h-[90vh] w-[560px] overflow-auto border border-border bg-panel shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold">Nuevo envío (ENV)</h2>
          <p className="text-[12px] text-muted">
            Código {codeQ.data ?? "…"} · snapshots + ZIP · solo si completo
          </p>
        </div>
        <div className="space-y-3 p-4">
          <Field label="Destinatario *">
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={counterpart}
              onChange={(e) => setCounterpart(e.target.value)}
            />
          </Field>
          <Field label="Asunto *">
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={subject}
              onChange={(e) => setSubject(e.target.value)}
            />
          </Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label="Fecha (DD-MM-YYYY)">
              <input
                className="h-[34px] w-full border border-border bg-elevated px-2.5"
                value={dateUi}
                onChange={(e) => setDateUi(e.target.value)}
              />
            </Field>
            <Field label="Medio">
              <select
                className="h-[34px] w-full border border-border bg-elevated px-2.5"
                value={medium}
                onChange={(e) => setMedium(e.target.value)}
              >
                <option value="email">Email</option>
                <option value="portal">Portal</option>
                <option value="usb">USB / físico</option>
                <option value="other">Otro</option>
              </select>
            </Field>
          </div>
          <Field label="Notas">
            <textarea
              className="min-h-[60px] w-full border border-border bg-elevated px-2.5 py-2"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
            />
          </Field>
          <div className="flex items-center gap-2">
            <Button variant="secondary" size="sm" onClick={addFiles}>
              <PackagePlus size={14} /> Añadir archivos…
            </Button>
            <span className="text-[12px] text-muted">{items.length} archivo(s)</span>
          </div>
          <ul className="max-h-[120px] overflow-auto text-[11px] text-muted">
            {items.map((it) => (
              <li key={it.absolute_path} className="truncate">
                {it.absolute_path}
              </li>
            ))}
          </ul>
          {preview && (
            <div
              className={cn(
                "border px-3 py-2 text-[12px]",
                preview.all_available
                  ? "border-success/40 bg-success/10"
                  : "border-warn/40 bg-warn/10 text-warn",
              )}
            >
              <div className="font-mono text-accent">{preview.code}</div>
              <div className="text-muted">{preview.folder_relative_path}</div>
              <ul className="mt-1">
                {preview.items.map((it) => (
                  <li key={it.absolute_path}>
                    {it.file_name}: {it.available ? "OK" : it.error}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {error && (
            <div className="border border-error/40 bg-error/10 px-3 py-2 text-[12px] text-error">
              {error}
            </div>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button
            variant="secondary"
            disabled={!counterpart || !subject || items.length === 0}
            onClick={() => previewMut.mutate()}
          >
            Validar / preview
          </Button>
          <Button
            disabled={
              !preview?.all_available ||
              createMut.isPending ||
              !counterpart ||
              !subject
            }
            onClick={() => createMut.mutate()}
          >
            {createMut.isPending ? "Generando ZIP…" : "Registrar envío"}
          </Button>
        </div>
      </div>
    </div>
  );
}

function ReceiveDialog({
  projectId,
  sentOptions,
  onClose,
  onCreated,
}: {
  projectId: string;
  sentOptions: { id: string; label: string }[];
  onClose: () => void;
  onCreated: (id: string) => Promise<void>;
}) {
  const [counterpart, setCounterpart] = useState("");
  const [subject, setSubject] = useState("Recepción de documentación");
  const [dateUi, setDateUi] = useState(todayUi());
  const [sourcePath, setSourcePath] = useState<string | null>(null);
  const [toAntecedentes, setToAntecedentes] = useState(true);
  const [linkSentId, setLinkSentId] = useState("");
  const [error, setError] = useState<string | null>(null);

  const codeQ = useQuery({
    queryKey: ["tx-code-rec", projectId],
    queryFn: () => api.previewNextTransmissionCode(projectId, "REC"),
  });

  const mut = useMutation({
    mutationFn: () =>
      api.importReceivedTransmission({
        project_id: projectId,
        counterpart,
        subject,
        received_at: uiDateToIso(dateUi) ?? todayIso(),
        source_path: sourcePath!,
        extract_to_antecedentes: toAntecedentes,
        link_to_sent_id: linkSentId || null,
      }),
    onSuccess: (tx) => onCreated(tx.id),
    onError: (e: Error) => setError(String(e)),
  });

  async function pickSource(kind: "file" | "dir") {
    const picked = await open({
      multiple: false,
      directory: kind === "dir",
      title: kind === "dir" ? "Carpeta recibida" : "ZIP o archivo recibido",
    });
    if (typeof picked === "string") setSourcePath(picked);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55">
      <div className="w-[520px] border border-border bg-panel shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <h2 className="text-[14px] font-semibold">Registrar recibido (REC)</h2>
          <p className="text-[12px] text-muted">
            Código {codeQ.data ?? "…"} · conserva original · ZIP Slip blocked
          </p>
        </div>
        <div className="space-y-3 p-4">
          <Field label="Remitente *">
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={counterpart}
              onChange={(e) => setCounterpart(e.target.value)}
            />
          </Field>
          <Field label="Asunto">
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={subject}
              onChange={(e) => setSubject(e.target.value)}
            />
          </Field>
          <Field label="Fecha recepción (DD-MM-YYYY)">
            <input
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={dateUi}
              onChange={(e) => setDateUi(e.target.value)}
            />
          </Field>
          <div className="flex gap-2">
            <Button variant="secondary" size="sm" onClick={() => pickSource("file")}>
              ZIP / archivo…
            </Button>
            <Button variant="secondary" size="sm" onClick={() => pickSource("dir")}>
              Carpeta…
            </Button>
          </div>
          <div className="truncate text-[11px] text-muted">
            {sourcePath ?? "Sin origen"}
          </div>
          <label className="flex items-center gap-2 text-[12.5px]">
            <input
              type="checkbox"
              checked={toAntecedentes}
              onChange={(e) => setToAntecedentes(e.target.checked)}
            />
            Extraer copia a Antecedentes (sin romper registro)
          </label>
          <Field label="Vincular a ENV (opcional)">
            <select
              className="h-[34px] w-full border border-border bg-elevated px-2.5"
              value={linkSentId}
              onChange={(e) => setLinkSentId(e.target.value)}
            >
              <option value="">Ninguno</option>
              {sentOptions.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </Field>
          {error && (
            <div className="border border-error/40 bg-error/10 px-3 py-2 text-[12px] text-error">
              {error}
            </div>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button
            disabled={!counterpart || !sourcePath || mut.isPending}
            onClick={() => mut.mutate()}
          >
            {mut.isPending ? "Importando…" : "Registrar recepción"}
          </Button>
        </div>
      </div>
    </div>
  );
}
