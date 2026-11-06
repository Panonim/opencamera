import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));

const { CHECK_INTERVAL_MS, isCheckDue } = await import("./updates");

describe("isCheckDue", () => {
  const now = 1_000_000_000_000;

  it("is due when never checked", () => expect(isCheckDue(0, now)).toBe(true));
  it("is not due right after a check", () => expect(isCheckDue(now - 1000, now)).toBe(false));
  it("is due once 3 days have passed", () => expect(isCheckDue(now - CHECK_INTERVAL_MS, now)).toBe(true));
  it("is not due just before 3 days", () => expect(isCheckDue(now - CHECK_INTERVAL_MS + 1, now)).toBe(false));
});
