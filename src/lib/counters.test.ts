import { describe, expect, it } from "vitest";
import { dialogClosed, dialogOpened, mergeChangedSince, recordChange } from "./counters";
import type { UndoReport } from "./types";

const report: UndoReport = {
  results: [
    { id: "a", outcomes: [{ kind: "changedSince", what: "ValueA" }] },
    { id: "b", outcomes: [{ kind: "restored" }] },
  ],
};

describe("mergeChangedSince", () => {
  it("notes_changed_since_and_clears_the_rest", () => {
    expect(mergeChangedSince({ b: "Old", c: "Keep" }, report)).toEqual({ a: "ValueA", c: "Keep" });
  });

  it("does_not_mutate_input", () => {
    const before = { b: "Old" };
    mergeChangedSince(before, report);
    expect(before).toEqual({ b: "Old" });
  });
});

describe("recordChange", () => {
  it("bumps_the_revision_every_time", () => {
    const h = { revision: 0, changedSince: {} };
    recordChange(h, null);
    recordChange(h, null);
    expect(h.revision).toBe(2);
    expect(h.changedSince).toEqual({});
  });

  it("merges_the_undo_report", () => {
    const h = { revision: 4, changedSince: { b: "Old" } };
    recordChange(h, report);
    expect(h).toEqual({ revision: 5, changedSince: { a: "ValueA" } });
  });
});

describe("dialog count", () => {
  it("counts_nested_dialogs_and_never_goes_negative", () => {
    const h = { dialogs: 0 };
    dialogOpened(h);
    dialogOpened(h);
    dialogClosed(h);
    expect(h.dialogs).toBe(1);
    dialogClosed(h);
    dialogClosed(h);
    expect(h.dialogs).toBe(0);
  });
});
