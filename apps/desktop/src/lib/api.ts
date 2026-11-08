import type { PhoneSettings, PhoneState, PhoneStats } from "@opencamera/protocol";
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Preset } from "./presets";

export type Phase = "idle" | "pairing" | "connecting" | "connected" | "reconnecting" | "failed" | "disconnected";
export type VirtualCameraStatus = "starting" | "active" | "needsSetup" | "unsupported" | "error";

export interface Device {
  id: string;
  name: string;
}

export interface EngineState {
  phase: Phase;
  error: { code: string; detail: string } | null;
  device: Device | null;
  phone: PhoneState | null;
  pairing: { url: string; expiresAt: number } | null;
  server: { running: boolean; address: string | null; port: number };
  virtualCamera: { status: VirtualCameraStatus; message: string | null };
}

export interface Stats {
  fps: number;
  bitrateKbps: number;
  packetLoss: number;
  droppedFrames: number;
  rttMs: number | null;
  width: number;
  height: number;
  quality: "good" | "fair" | "poor" | "none";
  phone: PhoneStats | null;
}

export interface Processing {
  mirror: boolean;
  rotation: 0 | 90 | 180 | 270;
  brightness: number;
  contrast: number;
  framing: "fit" | "fill";
  placeholder: "card" | "black" | "freeze";
  outputSize: "720p" | "1080p" | "2160p";
}

export interface Settings {
  theme: "system" | "light" | "dark";
  /** "system" or a locale code like "en". */
  language: string;
  autoReconnect: boolean;
  notifications: boolean;
  launchAtLogin: boolean;
  keepRunning: boolean;
  port: number;
  address: string;
  iceServer: string;
  processing: Processing;
  grid: boolean;
  presets: Preset[];
  onboarded: boolean;
  developerMode: boolean;
}

export interface Snapshot {
  state: EngineState;
  settings: Settings;
  version: string;
  platform: string;
  addresses: string[];
}

export interface Diagnostics {
  version: string;
  os: string;
  arch: string;
  addresses: string[];
  server: EngineState["server"];
  virtualCamera: EngineState["virtualCamera"];
  virtualCameraDetails: string;
  pairedDevices: number;
  logs: string[];
}

export const api = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  startPairing: () => invoke<{ url: string; expiresAt: number }>("start_pairing"),
  cancelPairing: () => invoke("cancel_pairing"),
  disconnect: () => invoke("disconnect"),
  clearError: () => invoke("clear_error"),
  setPhoneSettings: (patch: Partial<PhoneSettings>) => invoke("set_phone_settings", { patch }),
  diagnostics: () => invoke<Diagnostics>("diagnostics"),
  retryVirtualCamera: () => invoke("retry_virtual_camera"),
  setupVirtualCamera: () => invoke("setup_virtual_camera"),
  removeVirtualCamera: () => invoke("remove_virtual_camera"),
  updateSettings: (settings: Settings) => invoke("update_settings", { settings }),
  resetAll: () => invoke("reset_all"),
};

export function subscribePreview(onFrame: (jpeg: ArrayBuffer) => void) {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = onFrame;
  return invoke("subscribe_preview", { channel });
}

export function onEngineEvents(handlers: {
  state: (s: EngineState) => void;
  stats: (s: Stats) => void;
}) {
  return Promise.all([
    listen<EngineState>("engine-state", (e) => handlers.state(e.payload)),
    listen<Stats>("engine-stats", (e) => handlers.stats(e.payload)),
  ]);
}

export function onSettingsChanged(handler: (s: Settings) => void) {
  return listen<Settings>("settings-changed", (e) => handler(e.payload));
}

export function onEngineLog(handler: (line: string) => void) {
  return listen<string>("engine-log", (e) => handler(e.payload));
}
