import { describe, expect, it } from "vitest";
import { formatCountdown } from "./utils";

describe("formatCountdown", () => {
  it("pads seconds", () => expect(formatCountdown(65)).toBe("1:05"));
  it("handles zero", () => expect(formatCountdown(0)).toBe("0:00"));
  it("handles full minutes", () => expect(formatCountdown(300)).toBe("5:00"));
});
