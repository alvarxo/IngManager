import { describe, expect, it } from "vitest";
import {
  filterDocsByModelType,
  groupDocsByModelType,
  modelTypePrefix,
} from "./model-types";
import type { DocumentRow } from "./types";

function doc(partial: Partial<DocumentRow> & { id: string }): DocumentRow {
  return {
    project_id: "p",
    category_id: null,
    business_code: null,
    title: partial.id,
    doc_type: "MOD",
    model_type: null,
    current_version_id: null,
    status: "active",
    subcategory: null,
    origin: null,
    received_at: null,
    notes: null,
    metadata_json: null,
    created_at: "",
    updated_at: "",
    category_key: "modelos",
    current_version_number: null,
    current_version_label: null,
    file_count: 0,
    has_unregistered_change: 0,
    current_revision_code: null,
    current_revision_status: null,
    ...partial,
  };
}

describe("model type filter/group helpers", () => {
  const docs = [
    doc({ id: "1", model_type: "sap2000", business_code: "1126-SAP-001" }),
    doc({ id: "2", model_type: "revit", business_code: "1126-RVT-001" }),
    doc({ id: "3", model_type: "other", business_code: "1126-MOD-001" }),
    doc({ id: "4", model_type: null, business_code: "1126-MOD-002" }),
  ];

  it("filters by model_type", () => {
    expect(filterDocsByModelType(docs, "all")).toHaveLength(4);
    expect(filterDocsByModelType(docs, "sap2000").map((d) => d.id)).toEqual([
      "1",
    ]);
    expect(filterDocsByModelType(docs, "other").map((d) => d.id)).toEqual([
      "3",
      "4",
    ]);
  });

  it("groups by model_type with stable order", () => {
    const groups = groupDocsByModelType(docs);
    expect(groups.map((g) => g.modelType)).toEqual([
      "sap2000",
      "revit",
      "other",
    ]);
    expect(modelTypePrefix("sap2000")).toBe("SAP");
    expect(modelTypePrefix("idea_statica")).toBe("IDE");
  });
});
