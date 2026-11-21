import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("formats gigabytes with one decimal", () => {
    expect(formatBytes(6_657_199_308)).toBe("6.2 GB");
  });

  it("formats zero as bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("formats kilobytes with one decimal", () => {
    expect(formatBytes(1536)).toBe("1.5 KB");
  });
});
