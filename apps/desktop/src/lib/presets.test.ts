import { describe, expect, it } from "vitest";
import { MAX_PRESETS, addPreset, pickPhone, removePreset, type Preset } from "./presets";

const base = { mirror: false, rotation: 0, brightness: 0, contrast: 0, framing: "fit", placeholder: "card", outputSize: "1080p" } as const;
const make = (id: string, name = id): Preset => ({ id, name, phone: { resolution: "1080p", fps: 30, quality: "balanced", bitrate: "auto", batterySaver: true }, processing: base });

describe("presets", () => {
  it("replaces a preset with the same name", () => {
    const list = addPreset([make("a", "Meeting")], make("b", " meeting "));
    expect(list.map((p) => p.id)).toEqual(["b"]);
  });
  it("drops the oldest past the limit", () => {
    let list: Preset[] = [];
    for (let i = 0; i < MAX_PRESETS + 2; i++) list = addPreset(list, make(String(i)));
    expect(list).toHaveLength(MAX_PRESETS);
    expect(list[0].id).toBe("2");
  });
  it("removes by id", () => expect(removePreset([make("a"), make("b")], "a").map((p) => p.id)).toEqual(["b"]));
  it("keeps only camera-independent phone settings", () => {
    const phone = pickPhone({ deviceId: "x", facingMode: "user", resolution: "720p", fps: 60, quality: "low-latency", videoEnabled: true, bitrate: 4000, batterySaver: false });
    expect(phone).toEqual({ resolution: "720p", fps: 60, quality: "low-latency", bitrate: 4000, batterySaver: false });
  });
});
