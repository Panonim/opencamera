import { describe, expect, it } from "vitest";
import {
  ADAPT_START,
  SCALE_STEPS,
  WARNING_START,
  streamProblem,
  sustainedWarning,
  type StreamWarning,
  adaptScale,
  bitrateCeilingKbps,
  degradationPreference,
  orderCodecs,
  profileOf,
  throttledFps,
  maxBitrateKbps,
  parseDesktopMessage,
  readPairingToken,
  reconnectDelayMs,
  supportedFrameRates,
  supportedResolutions,
} from "./index";

describe("parseDesktopMessage", () => {
  it("accepts known message types", () => {
    expect(parseDesktopMessage('{"type":"answer","sdp":"v=0"}')).toEqual({ type: "answer", sdp: "v=0" });
  });
  it("rejects unknown types and invalid JSON", () => {
    expect(parseDesktopMessage('{"type":"evil"}')).toBeNull();
    expect(parseDesktopMessage("not json")).toBeNull();
    expect(parseDesktopMessage("null")).toBeNull();
  });
});

describe("readPairingToken", () => {
  it("reads token from fragment", () => {
    expect(readPairingToken("#t=abcdefghijklmnop_-123")).toBe("abcdefghijklmnop_-123");
  });
  it("rejects missing or malformed tokens", () => {
    expect(readPairingToken("")).toBeNull();
    expect(readPairingToken("#t=short")).toBeNull();
    expect(readPairingToken("#t=<script>alert(1)</script>")).toBeNull();
  });
});

describe("capabilities", () => {
  it("limits resolutions to camera maximum regardless of orientation", () => {
    expect(supportedResolutions(1080, 1920)).toEqual(["480p", "720p", "1080p"]);
    expect(supportedResolutions(4032, 3024)).toEqual(["480p", "720p", "1080p", "1440p", "2160p"]);
  });
  it("falls back to safe defaults when capabilities are unknown", () => {
    expect(supportedResolutions()).toEqual(["480p", "720p", "1080p"]);
    expect(supportedFrameRates()).toEqual([24, 30]);
  });
  it("limits frame rates", () => {
    expect(supportedFrameRates(30)).toEqual([24, 30]);
    expect(supportedFrameRates(59.94)).toEqual([24, 30, 60]);
  });
});

describe("bitrate", () => {
  it("scales with preset and fps", () => {
    expect(maxBitrateKbps("1080p", 30, "balanced")).toBe(5000);
    expect(maxBitrateKbps("1080p", 60, "balanced")).toBe(10000);
    expect(maxBitrateKbps("720p", 30, "low-latency")).toBe(1500);
  });
  it("uses manual value when set", () => {
    expect(bitrateCeilingKbps({ resolution: "1080p", quality: "balanced", bitrate: "auto" }, 30)).toBe(5000);
    expect(bitrateCeilingKbps({ resolution: "1080p", quality: "balanced", bitrate: 1234 }, 60)).toBe(1234);
  });
});

describe("codecs", () => {
  const vp8 = { mimeType: "video/VP8" };
  const cb = { mimeType: "video/H264", sdpFmtpLine: "packetization-mode=1;profile-level-id=42e01f" };
  const high = { mimeType: "video/H264", sdpFmtpLine: "packetization-mode=1;profile-level-id=640C1F" };
  it("reads profile", () => {
    expect(profileOf(cb.sdpFmtpLine)).toBe("baseline");
    expect(profileOf(high.sdpFmtpLine)).toBe("high");
    expect(profileOf("profile-level-id=4d001f")).toBe("main");
    expect(profileOf(undefined)).toBeNull();
  });
  it("puts Baseline first, then Main and High, and keeps every codec", () => {
    const main = { mimeType: "video/H264", sdpFmtpLine: "profile-level-id=4d001f" };
    expect(orderCodecs([vp8, high, main, cb])).toEqual([cb, main, high, vp8]);
  });
  it("still puts H.264 first when the phone has no Baseline", () => {
    expect(orderCodecs([vp8, high])).toEqual([high, vp8]);
  });
});

describe("battery saver", () => {
  const power = (battery: number | null, charging: boolean | null, thermal: null | "nominal" | "serious" = null) => ({ battery, charging, thermal });
  it("throttles on low battery when not charging", () => {
    expect(throttledFps(30, power(0.15, false), true)).toBe(15);
    expect(throttledFps(30, power(0.15, true), true)).toBeNull();
    expect(throttledFps(30, power(0.5, false), true)).toBeNull();
  });
  it("throttles when hot", () => {
    expect(throttledFps(60, power(null, null, "serious"), true)).toBe(15);
    expect(throttledFps(60, power(null, null, "nominal"), true)).toBeNull();
  });
  it("does nothing when off, unknown or already slow", () => {
    expect(throttledFps(30, power(0.1, false), false)).toBeNull();
    expect(throttledFps(30, undefined, true)).toBeNull();
    expect(throttledFps(15, power(0.1, false), true)).toBeNull();
  });
});

describe("degradationPreference", () => {
  it("keeps resolution on iOS unless low latency is chosen", () => {
    expect(degradationPreference("balanced")).toBe("balanced");
    expect(degradationPreference("balanced", true)).toBe("maintain-resolution");
    expect(degradationPreference("low-latency", true)).toBe("maintain-framerate");
  });
});

describe("adaptScale", () => {
  const at = (availableKbps: number | null) => ({ availableKbps, ceilingKbps: 5000 });
  const run = (samples: (number | null)[], from = ADAPT_START) => samples.map(at).reduce(adaptScale, from);
  it("shrinks only after three short samples in a row", () => {
    expect(run([1000, 1000]).step).toBe(0);
    expect(run([1000, 1000, 1000]).step).toBe(1);
    expect(run([1000, 5000, 1000, 1000]).step).toBe(0);
  });
  it("ignores missing estimates and stays at full size on a fast link", () => {
    expect(run([null, null, null, null])).toEqual(ADAPT_START);
    expect(run(Array(20).fill(8000)).step).toBe(0);
  });
  it("holds steady between the shrink and grow thresholds", () => {
    const shrunk = run([2000, 2000, 2000]);
    expect(shrunk.step).toBe(1);
    expect(run(Array(20).fill(3500), shrunk).step).toBe(1);
  });
  it("grows back after five samples with headroom and stays within bounds", () => {
    const shrunk = run([2000, 2000, 2000]);
    expect(run([4000, 4000, 4000, 4000], shrunk).step).toBe(1);
    expect(run([4000, 4000, 4000, 4000, 4000], shrunk).step).toBe(0);
    expect(run(Array(30).fill(50)).step).toBe(SCALE_STEPS.length - 1);
  });
});

describe("stream warnings", () => {
  it("names heat first, then processor or Wi-Fi limits", () => {
    expect(streamProblem("critical", "bandwidth")).toBe("hot");
    expect(streamProblem("fair", "cpu")).toBe("cpu");
    expect(streamProblem(null, "bandwidth")).toBe("bandwidth");
    expect(streamProblem("nominal", "other")).toBeNull();
    expect(streamProblem(undefined, undefined)).toBeNull();
  });
  const run = (samples: (StreamWarning | null)[], from = WARNING_START) =>
    samples.reduce((s, p, i) => sustainedWarning(s, p, i * 2000), from);
  it("shows a problem only once it lasts", () => {
    expect(run(["cpu", "cpu", "cpu", "cpu", "cpu"]).shown).toBeNull();
    expect(run(Array(6).fill("cpu")).shown).toBe("cpu");
    expect(run(["cpu", "cpu", "cpu", null, "cpu", "cpu", "cpu"]).shown).toBeNull();
  });
  it("clears only once the problem has been gone a while", () => {
    const shown = run(Array(6).fill("bandwidth"));
    expect(run([null, null, null, null, null], shown).shown).toBe("bandwidth");
    expect(run(Array(6).fill(null), shown).shown).toBeNull();
  });
});

describe("reconnectDelayMs", () => {
  it("backs off exponentially with a cap", () => {
    expect(reconnectDelayMs(0)).toBe(500);
    expect(reconnectDelayMs(3)).toBe(4000);
    expect(reconnectDelayMs(20)).toBe(10000);
  });
});
