import type { FrameRate, H264Profile, PhonePower, PhoneSettings, QualityPreset, Resolution, ThermalState } from "./messages";

export const RESOLUTIONS: Record<Resolution, { width: number; height: number }> = {
  "480p": { width: 854, height: 480 },
  "720p": { width: 1280, height: 720 },
  "1080p": { width: 1920, height: 1080 },
  "1440p": { width: 2560, height: 1440 },
  "2160p": { width: 3840, height: 2160 },
};

export const ALL_RESOLUTIONS = Object.keys(RESOLUTIONS) as Resolution[];
export const ALL_FRAME_RATES: FrameRate[] = [24, 30, 60];

/** Lists the video sizes the camera can handle, whichever way the phone is held. */
export function supportedResolutions(maxWidth?: number, maxHeight?: number): Resolution[] {
  if (!maxWidth || !maxHeight) return ["480p", "720p", "1080p"];
  const long = Math.max(maxWidth, maxHeight);
  const short = Math.min(maxWidth, maxHeight);
  return ALL_RESOLUTIONS.filter((r) => RESOLUTIONS[r].width <= long && RESOLUTIONS[r].height <= short);
}

export function supportedFrameRates(maxFps?: number): FrameRate[] {
  // Ignores missing or unrealistically low frame rate limits that some cameras report.
  if (!maxFps || maxFps < 24) return [24, 30];
  return ALL_FRAME_RATES.filter((f) => f <= Math.round(maxFps));
}

const BASE_KBPS: Record<Resolution, number> = {
  "480p": 1200,
  "720p": 2500,
  "1080p": 5000,
  "1440p": 9000,
  "2160p": 16000,
};

const PRESET_FACTOR: Record<QualityPreset, number> = {
  "low-latency": 0.6,
  balanced: 1,
  "high-quality": 1.6,
};

/** Sets the most video data per second allowed, a slow network may use less. */
export function maxBitrateKbps(resolution: Resolution, fps: FrameRate, quality: QualityPreset): number {
  return Math.round(BASE_KBPS[resolution] * PRESET_FACTOR[quality] * (fps / 30));
}

/** iOS keeps resolution unless low latency is asked for, since its balanced mode turns the picture blurry and blocky. */
export function degradationPreference(quality: QualityPreset, ios = false): RTCDegradationPreference {
  if (quality === "low-latency") return "maintain-framerate";
  if (quality === "high-quality" || ios) return "maintain-resolution";
  return "balanced";
}

export const MANUAL_BITRATE = { min: 500, max: 20000, step: 250, default: 5000 };

export function bitrateCeilingKbps(s: Pick<PhoneSettings, "resolution" | "quality" | "bitrate">, fps: number): number {
  return s.bitrate === "auto" ? maxBitrateKbps(s.resolution, fps as FrameRate, s.quality) : s.bitrate;
}

/** Reads the H.264 profile from a codec's format line, such as profile-level-id=640c1f. */
export function profileOf(fmtp: string | undefined): H264Profile | null {
  const idc = /profile-level-id=([0-9a-f]{2})/i.exec(fmtp ?? "")?.[1].toLowerCase();
  return idc === "42" ? "baseline" : idc === "4d" ? "main" : idc === "64" ? "high" : null;
}

/** Puts H.264 first, the only format the desktop decodes, with Baseline ahead since it decodes cleanest. */
export function orderCodecs<T extends { mimeType: string; sdpFmtpLine?: string }>(codecs: T[]): T[] {
  const h264 = (p: H264Profile) => codecs.filter((c) => c.mimeType === "video/H264" && profileOf(c.sdpFmtpLine) === p);
  const first = [...h264("baseline"), ...h264("main"), ...h264("high")];
  return [...first, ...codecs.filter((c) => !first.includes(c))];
}

export const SAVER_FPS = 15;

/** Gives the lower frame rate the battery saver uses right now, or null when it is not needed. */
export function throttledFps(fps: number, power: PhonePower | undefined, saver: boolean): number | null {
  if (!saver || !power || fps <= SAVER_FPS) return null;
  const lowBattery = power.battery !== null && power.battery <= 0.2 && power.charging === false;
  const hot = power.thermal === "serious" || power.thermal === "critical";
  return lowBattery || hot ? SAVER_FPS : null;
}

export type StreamWarning = "hot" | "cpu" | "bandwidth";

/** Names what is holding the phone back right now, from its heat and the reason WebRTC gives for lowering quality. */
export function streamProblem(thermal: ThermalState | null | undefined, limitation: string | null | undefined): StreamWarning | null {
  if (thermal === "serious" || thermal === "critical") return "hot";
  return limitation === "cpu" || limitation === "bandwidth" ? limitation : null;
}

export interface WarningState {
  shown: StreamWarning | null;
  next: StreamWarning | null;
  since: number;
}

export const WARNING_START: WarningState = { shown: null, next: null, since: 0 };
export const WARNING_SETTLE_MS = 10_000;

/** Switches the shown warning only once a problem, or its absence, has lasted 10 seconds, so single samples don't flicker. */
export function sustainedWarning(s: WarningState, problem: StreamWarning | null, now: number): WarningState {
  if (problem !== s.next) return { ...s, next: problem, since: now };
  return problem !== s.shown && now - s.since >= WARNING_SETTLE_MS ? { ...s, shown: problem } : s;
}

export const SCALE_STEPS = [1, 1.5, 2, 3];

export interface AdaptState {
  step: number;
  bad: number;
  good: number;
}

export const ADAPT_START: AdaptState = { step: 0, bad: 0, good: 0 };

/** Share of the bitrate ceiling below which a picture size starts to look blocky. */
const CLEAN_SHARE = 0.6;
const kbpsNeeded = (ceilingKbps: number, step: number) => (ceilingKbps * CLEAN_SHARE) / SCALE_STEPS[step] ** 2;

/** Picks the picture size from the browser's bandwidth estimate: shrinks after 3 short samples in a row, grows after 5 with 30% headroom. */
export function adaptScale(s: AdaptState, sample: { availableKbps: number | null; ceilingKbps: number }): AdaptState {
  const { availableKbps: kbps, ceilingKbps } = sample;
  if (kbps == null) return s;
  if (s.step < SCALE_STEPS.length - 1 && kbps < kbpsNeeded(ceilingKbps, s.step)) {
    return s.bad + 1 >= 3 ? { step: s.step + 1, bad: 0, good: 0 } : { ...s, bad: s.bad + 1, good: 0 };
  }
  if (s.step > 0 && kbps >= kbpsNeeded(ceilingKbps, s.step - 1) * 1.3) {
    return s.good + 1 >= 5 ? { step: s.step - 1, bad: 0, good: 0 } : { ...s, good: s.good + 1, bad: 0 };
  }
  return { ...s, bad: 0, good: 0 };
}
