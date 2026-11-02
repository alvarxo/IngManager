import { create } from "zustand";

type View =
  | { kind: "home" }
  | { kind: "project"; projectId: string; categoryKey?: string }
  | { kind: "settings" };

type InspectorTab =
  | "project"
  | "file"
  | "trash"
  | "doc-detail"
  | "doc-versions"
  | "doc-related";

type ProjectContentMode = "documents" | "files";
type ProjectMainTab = "documents" | "transmissions";

interface UiState {
  view: View;
  inspectorOpen: boolean;
  showArchived: boolean;
  indexMessage: string | null;
  selectedPaths: string[];
  selectedDocumentId: string | null;
  selectedTransmissionId: string | null;
  inspectorTab: InspectorTab;
  projectContentMode: ProjectContentMode;
  projectMainTab: ProjectMainTab;
  showCurrentVersionOnly: boolean;
  setView: (view: View) => void;
  setInspectorOpen: (open: boolean) => void;
  setShowArchived: (show: boolean) => void;
  setIndexMessage: (msg: string | null) => void;
  setSelectedPaths: (paths: string[]) => void;
  setSelectedDocumentId: (id: string | null) => void;
  setSelectedTransmissionId: (id: string | null) => void;
  setInspectorTab: (tab: InspectorTab) => void;
  setProjectContentMode: (mode: ProjectContentMode) => void;
  setProjectMainTab: (tab: ProjectMainTab) => void;
  setShowCurrentVersionOnly: (value: boolean) => void;
}

export const useUiStore = create<UiState>((set) => ({
  view: { kind: "home" },
  inspectorOpen: true,
  showArchived: false,
  indexMessage: null,
  selectedPaths: [],
  selectedDocumentId: null,
  selectedTransmissionId: null,
  inspectorTab: "project",
  projectContentMode: "documents",
  projectMainTab: "documents",
  showCurrentVersionOnly: true,
  setView: (view) => set({ view }),
  setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
  setShowArchived: (showArchived) => set({ showArchived }),
  setIndexMessage: (indexMessage) => set({ indexMessage }),
  setSelectedPaths: (selectedPaths) => set({ selectedPaths }),
  setSelectedDocumentId: (selectedDocumentId) => set({ selectedDocumentId }),
  setSelectedTransmissionId: (selectedTransmissionId) =>
    set({ selectedTransmissionId }),
  setInspectorTab: (inspectorTab) => set({ inspectorTab }),
  setProjectContentMode: (projectContentMode) => set({ projectContentMode }),
  setProjectMainTab: (projectMainTab) => set({ projectMainTab }),
  setShowCurrentVersionOnly: (showCurrentVersionOnly) =>
    set({ showCurrentVersionOnly }),
}));
