import type { PhoneSettings } from "@opencamera/protocol";
import type { Processing } from "./api";

export type PresetPhone = Pick<PhoneSettings, "resolution" | "fps" | "quality" | "bitrate" | "batterySaver">;

export interface Preset {
  id: string;
  name: string;
  phone: PresetPhone;
  processing: Processing;
}

export const MAX_PRESETS = 8;

/** Keeps only the phone settings a preset stores, so it works on any camera. */
export function pickPhone(s: PhoneSettings): PresetPhone {
  return { resolution: s.resolution, fps: s.fps, quality: s.quality, bitrate: s.bitrate, batterySaver: s.batterySaver };
}

/** Adds a preset, replacing one with the same name and dropping the oldest past the limit. */
export function addPreset(list: Preset[], preset: Preset): Preset[] {
  const name = preset.name.trim().toLowerCase();
  return [...list.filter((p) => p.name.trim().toLowerCase() !== name), preset].slice(-MAX_PRESETS);
}

export const removePreset = (list: Preset[], id: string) => list.filter((p) => p.id !== id);
