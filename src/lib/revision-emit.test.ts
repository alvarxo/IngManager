import { describe, expect, it } from "vitest";
import {
  formatRevisionEmitMessage,
  planModelSpawn,
} from "./revision-emit";

describe("planModelSpawn", () => {
  const models = [
    { id: "m1", business_code: "1126-SAP-001", title: "A" },
    { id: "m2", business_code: "1126-SAP-002", title: "B" },
  ];

  it("Cancel / No → emit document only, never requires models", () => {
    expect(
      planModelSpawn({
        userWantsModel: false,
        documentId: "doc1",
        models,
        related: [],
      }),
    ).toEqual({ alsoCreateModel: false, sourceModelIds: null });
  });

  it("Sí with no models → document only + notice", () => {
    const plan = planModelSpawn({
      userWantsModel: true,
      documentId: "doc1",
      models: [],
      related: [],
    });
    expect(plan.alsoCreateModel).toBe(false);
    expect(plan.sourceModelIds).toBeNull();
    expect(plan.notice).toMatch(/solo el documento/);
  });

  it("Sí with single model → spawn that model", () => {
    expect(
      planModelSpawn({
        userWantsModel: true,
        documentId: "doc1",
        models: [models[0]],
        related: [],
      }),
    ).toEqual({
      alsoCreateModel: true,
      sourceModelIds: ["m1"],
    });
  });

  it("Sí with linked model prefers link over project list", () => {
    expect(
      planModelSpawn({
        userWantsModel: true,
        documentId: "doc1",
        models,
        related: [
          {
            relation_type: "modelo_de",
            source_document_id: "m2",
            target_document_id: "doc1",
          },
        ],
      }),
    ).toEqual({
      alsoCreateModel: true,
      sourceModelIds: ["m2"],
    });
  });

  it("Sí with ambiguous models and invalid pick → document only", () => {
    const plan = planModelSpawn({
      userWantsModel: true,
      documentId: "doc1",
      models,
      related: [],
      pickedIndex1Based: null,
    });
    expect(plan.alsoCreateModel).toBe(false);
    expect(plan.notice).toMatch(/solo el documento/);
  });

  it("Sí with ambiguous models and valid pick → spawn chosen", () => {
    expect(
      planModelSpawn({
        userWantsModel: true,
        documentId: "doc1",
        models,
        related: [],
        pickedIndex1Based: 2,
      }),
    ).toEqual({
      alsoCreateModel: true,
      sourceModelIds: ["m2"],
    });
  });
});

describe("formatRevisionEmitMessage", () => {
  it("silent when no spawn and no warnings", () => {
    expect(
      formatRevisionEmitMessage({
        spawned_model_document_ids: [],
        model_spawn_warnings: [],
      }),
    ).toBeNull();
  });

  it("reports spawned models", () => {
    expect(
      formatRevisionEmitMessage({
        spawned_model_document_ids: ["a", "b"],
        model_spawn_warnings: [],
      }),
    ).toBe("Revisión emitida. Modelo(s) creado(s): 2.");
  });

  it("reports soft model warnings without hiding success", () => {
    expect(
      formatRevisionEmitMessage({
        spawned_model_document_ids: [],
        model_spawn_warnings: ["falló el modelo"],
      }),
    ).toBe("Revisión emitida. falló el modelo");
  });
});
