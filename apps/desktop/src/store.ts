import { create } from "zustand";
import { api, onEngineEvents, onSettingsChanged, subscribePreview, type EngineState, type Settings, type Stats } from "./lib/api";

export type Page = "dashboard" | "camera" | "settings" | "about";

interface AppStore {
  ready: boolean;
  page: Page;
  pairOpen: boolean;
  state: EngineState | null;
  stats: Stats | null;
  settings: Settings | null;
  version: string;
  platform: string;
  addresses: string[];
  setPage: (page: Page) => void;
  setPairOpen: (open: boolean) => void;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;
  resetAll: () => Promise<void>;
}

export const useApp = create<AppStore>((set, get) => ({
  ready: false,
  page: "dashboard",
  pairOpen: false,
  state: null,
  stats: null,
  settings: null,
  version: "",
  platform: "",
  addresses: [],
  setPage: (page) => set({ page }),
  setPairOpen: (pairOpen) => set({ pairOpen }),
  saveSettings: async (patch) => {
    const previous = get().settings;
    if (!previous) return;
    const next = { ...previous, ...patch };
    set({ settings: next });
    try {
      await api.updateSettings(next);
    } catch (e) {
      set({ settings: previous });
      throw e;
    }
  },
  resetAll: async () => {
    await api.resetAll();
    const { settings, state } = await api.snapshot();
    set({ settings, state, page: "dashboard" });
  },
}));

// Holds the callbacks that receive preview frames.
const frameListeners = new Set<(jpeg: ArrayBuffer) => void>();
export function onPreviewFrame(fn: (jpeg: ArrayBuffer) => void) {
  frameListeners.add(fn);
  return () => {
    frameListeners.delete(fn);
  };
}

export async function initStore() {
  await onEngineEvents({
    state: (state) => {
      useApp.setState({ state });
      // Closes the pairing dialog once a phone connects.
      if (state.phase === "connecting" || state.phase === "connected") useApp.setState({ pairOpen: false });
    },
    stats: (stats) => useApp.setState({ stats }),
  });
  await onSettingsChanged((settings) => useApp.setState({ settings }));
  await subscribePreview((jpeg) => frameListeners.forEach((fn) => fn(jpeg)));
  const snap = await api.snapshot();
  useApp.setState({ ...snap, ready: true });
}
