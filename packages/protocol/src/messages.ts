export const PROTOCOL_VERSION = 1;

export type FacingMode = "user" | "environment";
export type Resolution = "480p" | "720p" | "1080p" | "1440p" | "2160p";
export type FrameRate = 24 | 30 | 60;
export type QualityPreset = "low-latency" | "balanced" | "high-quality";
export type H264Profile = "baseline" | "main" | "high";
/** Most video data per second in kbps, or "auto" to adapt to the network. */
export type BitrateSetting = "auto" | number;
export type ThermalState = "nominal" | "fair" | "serious" | "critical";

export interface PhonePower {
  /** Charge level from 0 to 1. */
  battery: number | null;
  charging: boolean | null;
  thermal: ThermalState | null;
}

export interface CameraInfo {
  deviceId: string;
  label: string;
  facing?: FacingMode;
}

/** Camera and video quality settings used on the phone. */
export interface PhoneSettings {
  deviceId?: string;
  facingMode: FacingMode;
  resolution: Resolution;
  fps: FrameRate;
  quality: QualityPreset;
  videoEnabled: boolean;
  bitrate: BitrateSetting;
  /** Lowers the frame rate when the battery is low or the phone runs hot. */
  batterySaver: boolean;
}

export interface PhoneCapabilities {
  resolutions: Resolution[];
  frameRates: FrameRate[];
  canSwitchCamera: boolean;
}

export interface PhoneState {
  cameras: CameraInfo[];
  settings: PhoneSettings;
  capabilities: PhoneCapabilities;
  /** Real size of the video the camera is capturing. */
  width: number;
  height: number;
  /** True while the page is hidden and the phone may pause the camera. */
  suspended: boolean;
  power: PhonePower;
}

export interface PhoneStats {
  bitrateKbps: number;
  encodeFps: number;
  rttMs: number | null;
  qualityLimitation: string;
  profile: H264Profile | null;
  /** Encoder the browser picked, such as MediaCodec or OpenH264. */
  encoder: string | null;
  hardware: boolean | null;
  /** How much the picture is shrunk to fit the network, 1 meaning full size. */
  scale: number;
}

export interface IceServer {
  urls: string[];
  username?: string;
  credential?: string;
}

export type IceCandidate = {
  candidate: string;
  sdpMid?: string | null;
  sdpMLineIndex?: number | null;
} | null;

export type PhoneMessage =
  | {
      type: "hello";
      protocolVersion: number;
      token?: string;
      deviceId?: string;
      deviceSecret?: string;
      deviceName: string;
      userAgent: string;
    }
  | { type: "offer"; sdp: string }
  | { type: "ice"; candidate: IceCandidate }
  | { type: "state"; state: PhoneState }
  | { type: "stats"; stats: PhoneStats }
  | { type: "bye" };

export type RejectReason =
  | "invalid_token"
  | "expired_token"
  | "unknown_device"
  | "busy"
  | "version_mismatch"
  | "replaced";

export type DesktopMessage =
  | {
      type: "welcome";
      deviceId: string;
      /** Sent only on first pairing so the phone can save it and reconnect later. */
      deviceSecret?: string;
      desktopName: string;
      iceServers: IceServer[];
    }
  | { type: "reject"; reason: RejectReason }
  | { type: "answer"; sdp: string }
  | { type: "ice"; candidate: IceCandidate }
  | { type: "control"; settings: Partial<PhoneSettings> }
  | { type: "bye"; reason: string };

const DESKTOP_TYPES = new Set(["welcome", "reject", "answer", "ice", "control", "bye"]);

export function parseDesktopMessage(raw: string): DesktopMessage | null {
  try {
    const msg = JSON.parse(raw);
    return msg && typeof msg === "object" && DESKTOP_TYPES.has(msg.type) ? (msg as DesktopMessage) : null;
  } catch {
    return null;
  }
}
