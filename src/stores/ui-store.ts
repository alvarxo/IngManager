import { create } from "zustand";

type View =
  | { kind: "home" }
  | { kind: "project"; projectId: string; categoryKey?: string }
  | { kind: "settings" };

interface UiState {
  view: View;
  inspectorOpen: boolean;
  showArchived: boolean;
  indexMessage: string | null;
  setView: (view: View) => void;
  setInspectorOpen: (open: boolean) => void;
  setShowArchived: (show: boolean) => void;
  setIndexMessage: (msg: string | null) => void;
}

export const useUiStore = create<UiState>((set) => ({
  view: { kind: "home" },
  inspectorOpen: true,
  showArchived: false,
  indexMessage: null,
  setView: (view) => set({ view }),
  setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
  setShowArchived: (showArchived) => set({ showArchived }),
  setIndexMessage: (indexMessage) => set({ indexMessage }),
}));
