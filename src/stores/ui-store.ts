import { create } from "zustand";

type View =
  | { kind: "home" }
  | { kind: "project"; projectId: string; categoryKey?: string }
  | { kind: "settings" };

type InspectorTab = "project" | "file" | "trash";

interface UiState {
  view: View;
  inspectorOpen: boolean;
  showArchived: boolean;
  indexMessage: string | null;
  selectedPaths: string[];
  inspectorTab: InspectorTab;
  setView: (view: View) => void;
  setInspectorOpen: (open: boolean) => void;
  setShowArchived: (show: boolean) => void;
  setIndexMessage: (msg: string | null) => void;
  setSelectedPaths: (paths: string[]) => void;
  setInspectorTab: (tab: InspectorTab) => void;
}

export const useUiStore = create<UiState>((set) => ({
  view: { kind: "home" },
  inspectorOpen: true,
  showArchived: false,
  indexMessage: null,
  selectedPaths: [],
  inspectorTab: "project",
  setView: (view) => set({ view }),
  setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
  setShowArchived: (showArchived) => set({ showArchived }),
  setIndexMessage: (indexMessage) => set({ indexMessage }),
  setSelectedPaths: (selectedPaths) => set({ selectedPaths }),
  setInspectorTab: (inspectorTab) => set({ inspectorTab }),
}));
