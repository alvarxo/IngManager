import type { DocumentRow, ModelType } from "./types";

export const MODEL_TYPE_OPTIONS: Array<{
  value: ModelType | "all";
  label: string;
  prefix: string;
}> = [
  { value: "all", label: "Todos los tipos", prefix: "" },
  { value: "sap2000", label: "SAP2000", prefix: "SAP" },
  { value: "revit", label: "Revit", prefix: "RVT" },
  { value: "idea_statica", label: "IDEA StatiCa", prefix: "IDE" },
  { value: "robot", label: "Robot", prefix: "ROB" },
  { value: "etabs", label: "ETABS", prefix: "ETB" },
  { value: "other", label: "Otro", prefix: "MOD" },
];

export function modelTypeLabel(modelType: string | null | undefined): string {
  if (!modelType) return "—";
  const hit = MODEL_TYPE_OPTIONS.find((o) => o.value === modelType);
  return hit?.label ?? modelType;
}

export function modelTypePrefix(modelType: string | null | undefined): string {
  if (!modelType) return "MOD";
  const hit = MODEL_TYPE_OPTIONS.find((o) => o.value === modelType);
  return hit?.prefix || "MOD";
}

/** Filter Modelos docs by subtype (`all` = no filter). */
export function filterDocsByModelType(
  docs: DocumentRow[],
  filter: ModelType | "all",
): DocumentRow[] {
  if (filter === "all") return docs;
  return docs.filter((d) => (d.model_type ?? "other") === filter);
}

/** Group docs by model_type; groups ordered SAP→RVT→IDE→ROB→ETB→other→unknown. */
export function groupDocsByModelType(
  docs: DocumentRow[],
): Array<{ modelType: string; label: string; docs: DocumentRow[] }> {
  const order = MODEL_TYPE_OPTIONS.filter((o) => o.value !== "all").map(
    (o) => o.value as string,
  );
  const map = new Map<string, DocumentRow[]>();
  for (const d of docs) {
    const key = d.model_type ?? "other";
    const list = map.get(key) ?? [];
    list.push(d);
    map.set(key, list);
  }
  const keys = [...map.keys()].sort((a, b) => {
    const ia = order.indexOf(a);
    const ib = order.indexOf(b);
    const sa = ia === -1 ? 999 : ia;
    const sb = ib === -1 ? 999 : ib;
    return sa - sb || a.localeCompare(b);
  });
  return keys.map((key) => ({
    modelType: key,
    label: modelTypeLabel(key),
    docs: map.get(key) ?? [],
  }));
}
