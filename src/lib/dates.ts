/** UI display DD-MM-YYYY ↔ ISO YYYY-MM-DD for storage */

export function isoToUiDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  return `${m[3]}-${m[2]}-${m[1]}`;
}

export function uiDateToIso(ui: string): string | null {
  const m = /^(\d{2})-(\d{2})-(\d{4})$/.exec(ui.trim());
  if (!m) return null;
  return `${m[3]}-${m[2]}-${m[1]}`;
}

export function todayIso(): string {
  const d = new Date();
  const y = d.getFullYear();
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${mo}-${day}`;
}

export function todayUi(): string {
  return isoToUiDate(todayIso());
}
