import type { CameraInfo, PhoneCapabilities, PhonePower, PhoneSettings, StreamWarning } from "@opencamera/protocol";
import { create } from "zustand";

export type Screen = "intro" | "starting" | "live" | "message";
export type LinkStatus = "connecting" | "connected" | "reconnecting";

export type MessageKind =
  | "no_pairing"
  | "permission_denied"
  | "no_camera"
  | "camera_busy"
  | "unsupported"
  | "unknown"
  | "invalid_token"
  | "expired_token"
  | "unknown_device"
  | "busy"
  | "version_mismatch"
  | "replaced"
  | "desktop_disconnect"
  | "user_disconnect"
  | "unreachable";

export interface MobileState {
  screen: Screen;
  message: MessageKind | null;
  status: LinkStatus;
  desktopName: string | null;
  stream: MediaStream | null;
  settings: PhoneSettings;
  cameras: CameraInfo[];
  capabilities: PhoneCapabilities;
  actual: { width: number; height: number; fps: number };
  /** True when the phone stopped the camera while the browser was in the background. */
  wasSuspended: boolean;
  power: PhonePower;
  warning: StreamWarning | null;
}

export const DEFAULT_SETTINGS: PhoneSettings = {
  facingMode: "environment",
  resolution: "1080p",
  fps: 30,
  quality: "balanced",
  videoEnabled: true,
  bitrate: "auto",
  batterySaver: true,
};

export const useMobile = create<MobileState>(() => ({
  screen: "intro",
  message: null,
  status: "connecting",
  desktopName: null,
  stream: null,
  settings: DEFAULT_SETTINGS,
  cameras: [],
  capabilities: { resolutions: ["480p", "720p", "1080p"], frameRates: [24, 30], canSwitchCamera: false },
  actual: { width: 0, height: 0, fps: 0 },
  wasSuspended: false,
  power: { battery: null, charging: null, thermal: null },
  warning: null,
}));

export const setMobile = useMobile.setState;
