import { create } from "zustand";

export type ClipboardMode = "copy" | "cut" | null;

interface ClipboardState {
  mode: ClipboardMode;
  paths: string[];
  setClipboard: (mode: Exclude<ClipboardMode, null>, paths: string[]) => void;
  clear: () => void;
}

export const useClipboardStore = create<ClipboardState>((set) => ({
  mode: null,
  paths: [],
  setClipboard: (mode, paths) => set({ mode, paths: [...paths] }),
  clear: () => set({ mode: null, paths: [] }),
}));
