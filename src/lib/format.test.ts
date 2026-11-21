import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("formats gigabytes with one decimal", () => {
    expect(formatBytes(6_657_199_308)).toBe("6.2 GB");
  });

  it("formats zero as bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("drops the decimal from 100 up and a trailing zero", () => {
    expect(formatBytes(146_800_640)).toBe("140 MB");
    expect(formatBytes(524_288_000)).toBe("500 MB");
    expect(formatBytes(1024 ** 3)).toBe("1 GB");
  });

  it("formats kilobytes with one decimal", () => {
    expect(formatBytes(1536)).toBe("1.5 KB");
  });
});
