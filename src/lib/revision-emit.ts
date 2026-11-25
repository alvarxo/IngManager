/** Helpers for «Emitir revisión» — model opt-in must never block the document. */

export type ModelCandidate = { id: string; business_code?: string | null; title: string };

export type RelatedEdge = {
  relation_type: string;
  source_document_id: string;
  target_document_id: string;
};

export type ModelSpawnPlan =
  | { alsoCreateModel: false; sourceModelIds: null; notice?: string }
  | { alsoCreateModel: true; sourceModelIds: string[]; notice?: string };

/**
 * Decide whether to request model spawn after the user answered the opt-in dialog.
 * Cancel / decline → never require models. Ambiguous / empty → emit document only.
 */
export function planModelSpawn(opts: {
  userWantsModel: boolean;
  documentId: string;
  models: ModelCandidate[];
  related: RelatedEdge[];
  /** 1-based index from a multi-model picker; null/invalid → document only. */
  pickedIndex1Based?: number | null;
}): ModelSpawnPlan {
  if (!opts.userWantsModel) {
    return { alsoCreateModel: false, sourceModelIds: null };
  }

  const linkedIds = new Set<string>();
  for (const r of opts.related) {
    if (r.relation_type !== "modelo_de" && r.relation_type !== "relacionado") {
      continue;
    }
    const other =
      r.source_document_id === opts.documentId
        ? r.target_document_id
        : r.source_document_id;
    if (opts.models.some((m) => m.id === other)) {
      linkedIds.add(other);
    }
  }

  const candidates =
    linkedIds.size > 0
      ? opts.models.filter((m) => linkedIds.has(m.id))
      : opts.models;

  if (candidates.length === 0) {
    return {
      alsoCreateModel: false,
      sourceModelIds: null,
      notice: "No hay modelos en el proyecto. Se emitirá solo el documento.",
    };
  }

  if (candidates.length === 1) {
    return {
      alsoCreateModel: true,
      sourceModelIds: [candidates[0].id],
    };
  }

  const idx =
    opts.pickedIndex1Based != null ? opts.pickedIndex1Based - 1 : -1;
  if (idx < 0 || idx >= candidates.length) {
    return {
      alsoCreateModel: false,
      sourceModelIds: null,
      notice: "Selección inválida. Se emitirá solo el documento.",
    };
  }

  return {
    alsoCreateModel: true,
    sourceModelIds: [candidates[idx].id],
  };
}

/** Format success / soft-warning message after create_revision returns. */
export function formatRevisionEmitMessage(result: {
  spawned_model_document_ids?: string[] | null;
  model_spawn_warnings?: string[] | null;
}): string | null {
  const warnings = result.model_spawn_warnings?.filter(Boolean) ?? [];
  const spawned = result.spawned_model_document_ids?.length ?? 0;
  if (warnings.length === 0 && spawned === 0) return null;
  const parts: string[] = ["Revisión emitida."];
  if (spawned > 0) {
    parts.push(`Modelo(s) creado(s): ${spawned}.`);
  }
  if (warnings.length > 0) {
    parts.push(warnings.join(" "));
  }
  return parts.join(" ");
}
