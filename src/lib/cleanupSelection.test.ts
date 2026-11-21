import { describe, expect, it } from "vitest";
import { selectedBytes, selectedTargets } from "./cleanupSelection";
import type { CleanupTarget } from "./types";

const t = (id: string, bytes: number): CleanupTarget => ({ id, label: id, paths: [], bytes, needsServiceStop: null });
const targets = [t("a", 100), t("b", 250), t("c", 0)];

describe("cleanup selection", () => {
  it("sums only selected targets", () => {
    expect(selectedBytes(targets, new Set(["a", "b"]))).toBe(350);
    expect(selectedBytes(targets, new Set())).toBe(0);
  });
  it("ignores unknown ids and keeps scan order", () => {
    expect(selectedTargets(targets, new Set(["c", "x", "a"])).map((x) => x.id)).toEqual(["a", "c"]);
  });
});
