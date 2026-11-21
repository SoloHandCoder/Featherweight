import { describe, expect, it } from "vitest";
import { nextSelection, selectedBytes, selectedTargets } from "./cleanupSelection";
import type { CleanupTarget } from "./types";

const t = (id: string, bytes: number): CleanupTarget => ({ id, label: id, paths: [], bytes, needsServiceStop: null });
const targets = [t("a", 100), t("b", 250), t("c", 0)];

describe("nextSelection", () => {
  it("checks non-empty targets by default and skips zero-byte ones", () => {
    expect([...nextSelection(targets, new Set(), new Set())]).toEqual(["a", "b"]);
  });
  it("keeps user-unchecked targets unchecked on rescan, checks new ones", () => {
    const next = nextSelection([...targets, t("d", 5)], new Set(["a", "b"]), new Set(["a"]));
    expect([...next].sort()).toEqual(["a", "d"]);
  });
});

describe("cleanup selection", () => {
  it("sums only selected targets", () => {
    expect(selectedBytes(targets, new Set(["a", "b"]))).toBe(350);
    expect(selectedBytes(targets, new Set())).toBe(0);
  });
  it("ignores unknown ids and keeps scan order", () => {
    expect(selectedTargets(targets, new Set(["c", "x", "a"])).map((x) => x.id)).toEqual(["a", "c"]);
  });
});
