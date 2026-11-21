import { describe, expect, it } from "vitest";
import { beginBusy, endBusy, type BusyHolder } from "./busy";

describe("busy guard", () => {
  it("begin_succeeds_when_idle_and_refuses_when_busy", () => {
    const h: BusyHolder = { busy: null };
    expect(beginBusy(h, { kind: "apply", label: "Applying" })).toBe(true);
    expect(h.busy).toEqual({ kind: "apply", label: "Applying" });
    expect(beginBusy(h, { kind: "undo", label: "Undoing" })).toBe(false);
    expect(h.busy?.kind).toBe("apply");
  });

  it("end_clears_so_the_next_begin_works", () => {
    const h: BusyHolder = { busy: null };
    beginBusy(h, { kind: "undo", label: "Undoing" });
    endBusy(h);
    expect(h.busy).toBeNull();
    expect(beginBusy(h, { kind: "cleanup", label: "Cleaning" })).toBe(true);
  });
});
