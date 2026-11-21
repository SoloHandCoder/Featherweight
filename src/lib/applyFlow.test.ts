import { describe, expect, it, vi } from "vitest";
import {
  appName,
  combineGroups,
  groupApplyReport,
  groupUndoReport,
  mergeUndoReport,
  OTHER_ACCOUNT_DETAIL,
  planApply,
  restorePointStep,
  resultHeadline,
  runApply,
  runPreset,
  storeLink,
  type ApplyDeps,
  type PresetDeps,
  type ResultGroup,
} from "./applyFlow";
import type { ApplyReport, CleanupTarget, RestorePointOutcome, TweakView } from "./types";

function tweak(id: string, irreversible = false, changes: string[] = []): TweakView {
  return {
    id,
    title: `Title ${id}`,
    description: "",
    section: "remove",
    level: "safe",
    irreversible,
    perUser: false,
    status: { kind: "notApplied" },
    changes,
    reinstallHint: null,
    allUsersOptional: false,
    exclusiveWith: null,
    presets: [],
  };
}

const tweaks = [tweak("ads"), tweak("junk", true), tweak("recall")];

function report(ids: string[]): ApplyReport {
  return { results: ids.map((id) => ({ id, outcome: { kind: "applied" } })) };
}

function deps(restore: RestorePointOutcome, overrides: Partial<ApplyDeps> = {}): ApplyDeps {
  return {
    tweaks,
    ensureRestorePoint: vi.fn(async () => restore),
    enableSystemRestore: vi.fn(async () => {}),
    applyTweaks: vi.fn(async (ids: string[]) => report(ids)),
    confirmIrreversible: vi.fn(async () => true),
    confirmRestoreIssue: vi.fn(async () => true),
    ...overrides,
  };
}

const targets: CleanupTarget[] = [
  { id: "temp", label: "Temporary files", paths: [], bytes: 1000, needsServiceStop: null },
  { id: "empty", label: "Empty cache", paths: [], bytes: 0, needsServiceStop: null },
];

function presetDeps(restore: RestorePointOutcome, overrides: Partial<PresetDeps> = {}): PresetDeps {
  return {
    ...deps(restore),
    scanCleanup: vi.fn(async () => targets),
    runCleanup: vi.fn(async (ids: string[]) => ({ freedBytes: ids.length * 1000, skippedFiles: 0 })),
    ...overrides,
  };
}

describe("runPreset", () => {
  it("cleanup_included_when_confirmed", async () => {
    const d = presetDeps({ kind: "created" });
    const r = await runPreset(["ads", "junk"], d);
    expect(d.confirmIrreversible).toHaveBeenCalledWith(["junk"], [targets[0]]);
    expect(d.applyTweaks).toHaveBeenCalledWith(["ads", "junk"], []);
    expect(d.runCleanup).toHaveBeenCalledWith(["temp"]);
    expect(r).toEqual({
      apply: report(["ads", "junk"]),
      cleanup: { freedBytes: 1000, skippedFiles: 0 },
      cleanupError: null,
    });
  });

  it("applies_before_cleaning", async () => {
    const order: string[] = [];
    const d = presetDeps(
      { kind: "created" },
      {
        applyTweaks: vi.fn(async (ids: string[]) => {
          order.push("apply");
          return report(ids);
        }),
        runCleanup: vi.fn(async () => {
          order.push("clean");
          return { freedBytes: 1, skippedFiles: 0 };
        }),
      },
    );
    await runPreset(["ads"], d);
    expect(order).toEqual(["apply", "clean"]);
  });

  it("asks_about_cleanup_even_without_irreversible_tweaks", async () => {
    const d = presetDeps({ kind: "created" });
    await runPreset(["ads"], d);
    expect(d.confirmIrreversible).toHaveBeenCalledWith([], [targets[0]]);
  });

  it("skip_these_skips_app_removal_and_cleanup", async () => {
    const d = presetDeps({ kind: "created" }, { confirmIrreversible: vi.fn(async () => false) });
    const r = await runPreset(["ads", "junk"], d);
    expect(d.applyTweaks).toHaveBeenCalledWith(["ads"], []);
    expect(d.runCleanup).not.toHaveBeenCalled();
    expect(r?.cleanup).toBeNull();
  });

  it("cleanup_not_run_when_restore_point_cancelled", async () => {
    const d = presetDeps({ kind: "failed", message: "x" }, { confirmRestoreIssue: vi.fn(async () => false) });
    expect(await runPreset(["ads"], d)).toBeNull();
    expect(d.scanCleanup).not.toHaveBeenCalled();
    expect(d.applyTweaks).not.toHaveBeenCalled();
    expect(d.runCleanup).not.toHaveBeenCalled();
  });

  it("does_not_ask_when_nothing_is_irreversible_or_dirty", async () => {
    const d = presetDeps({ kind: "created" }, { scanCleanup: vi.fn(async () => [targets[1]]) });
    const r = await runPreset(["ads"], d);
    expect(d.confirmIrreversible).not.toHaveBeenCalled();
    expect(d.runCleanup).not.toHaveBeenCalled();
    expect(r?.apply).toEqual(report(["ads"]));
  });

  it("cleanup_only_when_no_tweaks_are_left", async () => {
    const d = presetDeps({ kind: "created" });
    const r = await runPreset([], d);
    expect(d.applyTweaks).not.toHaveBeenCalled();
    expect(r).toEqual({ apply: null, cleanup: { freedBytes: 1000, skippedFiles: 0 }, cleanupError: null });
  });

  it("a_failed_scan_still_applies_the_tweaks", async () => {
    const d = presetDeps({ kind: "created" }, { scanCleanup: vi.fn(async () => Promise.reject(new Error("denied"))) });
    const r = await runPreset(["ads"], d);
    expect(r?.apply).toEqual(report(["ads"]));
    expect(r?.cleanup).toBeNull();
    expect(r?.cleanupError).toBe("Couldn't look for junk files: denied");
  });

  it("a_failed_cleanup_keeps_the_apply_report", async () => {
    const d = presetDeps({ kind: "created" }, { runCleanup: vi.fn(async () => Promise.reject(new Error("busy"))) });
    const r = await runPreset(["ads"], d);
    expect(r?.apply).toEqual(report(["ads"]));
    expect(r?.cleanupError).toBe("Couldn't clean up: busy");
  });
});

describe("planApply", () => {
  it("splits ids into reversible and irreversible, keeping order", () => {
    expect(planApply(["ads", "junk", "recall"], tweaks)).toEqual({
      reversible: ["ads", "recall"],
      irreversible: ["junk"],
    });
  });
});

describe("runApply", () => {
  it("irreversible_requires_confirmation", async () => {
    const d = deps({ kind: "created" });
    await runApply(["ads", "junk"], [], d);
    expect(d.confirmIrreversible).toHaveBeenCalledWith(["junk"]);
    expect(d.applyTweaks).toHaveBeenCalledWith(["ads", "junk"], []);
    expect(d.confirmRestoreIssue).not.toHaveBeenCalled();
  });

  it("does not ask about irreversible tweaks when there are none", async () => {
    const d = deps({ kind: "alreadyMadeThisSession" });
    await runApply(["ads", "recall"], [], d);
    expect(d.confirmIrreversible).not.toHaveBeenCalled();
    expect(d.applyTweaks).toHaveBeenCalledWith(["ads", "recall"], []);
  });

  it("makes the restore point before asking anything", async () => {
    const order: string[] = [];
    const d = deps({ kind: "created" }, {
      ensureRestorePoint: vi.fn(async () => {
        order.push("restore");
        return { kind: "created" } as RestorePointOutcome;
      }),
      confirmIrreversible: vi.fn(async () => {
        order.push("confirm");
        return true;
      }),
      applyTweaks: vi.fn(async (ids: string[]) => {
        order.push("apply");
        return report(ids);
      }),
    });
    await runApply(["junk"], [], d);
    expect(order).toEqual(["restore", "confirm", "apply"]);
  });

  it("cancel_irreversible_applies_only_reversible", async () => {
    const d = deps({ kind: "created" }, { confirmIrreversible: vi.fn(async () => false) });
    const r = await runApply(["ads", "junk", "recall"], ["junk", "ads"], d);
    expect(d.applyTweaks).toHaveBeenCalledWith(["ads", "recall"], ["ads"]);
    expect(r?.results.map((x) => x.id)).toEqual(["ads", "recall"]);
  });

  it("applies nothing when only irreversible tweaks were picked and skipped", async () => {
    const d = deps({ kind: "created" }, { confirmIrreversible: vi.fn(async () => false) });
    expect(await runApply(["junk"], [], d)).toBeNull();
    expect(d.applyTweaks).not.toHaveBeenCalled();
  });

  it("throttled_restore_point_asks_then_continues", async () => {
    const d = deps({ kind: "throttled" });
    const r = await runApply(["ads"], [], d);
    expect(d.confirmRestoreIssue).toHaveBeenCalledWith({ kind: "throttled" }, { canEnable: false });
    expect(d.applyTweaks).toHaveBeenCalledWith(["ads"], []);
    expect(r).not.toBeNull();
  });

  it("cancelling at the restore point question applies nothing", async () => {
    const d = deps({ kind: "failed", message: "boom" }, { confirmRestoreIssue: vi.fn(async () => false) });
    expect(await runApply(["ads", "junk"], [], d)).toBeNull();
    expect(d.confirmIrreversible).not.toHaveBeenCalled();
    expect(d.applyTweaks).not.toHaveBeenCalled();
  });

  it("disabled_restore_point_offers_enable", async () => {
    const ensure = vi
      .fn<() => Promise<RestorePointOutcome>>()
      .mockResolvedValueOnce({ kind: "disabled" })
      .mockResolvedValueOnce({ kind: "created" });
    const d = deps({ kind: "disabled" }, {
      ensureRestorePoint: ensure,
      confirmRestoreIssue: vi.fn(async () => "enable" as const),
    });
    await runApply(["ads"], [], d);
    expect(d.confirmRestoreIssue).toHaveBeenCalledTimes(1);
    expect(d.confirmRestoreIssue).toHaveBeenCalledWith({ kind: "disabled" }, { canEnable: true });
    expect(d.enableSystemRestore).toHaveBeenCalledTimes(1);
    expect(ensure).toHaveBeenCalledTimes(2);
    expect(d.applyTweaks).toHaveBeenCalledWith(["ads"], []);
  });

  it("asks again without the enable offer when turning System Restore on fails", async () => {
    const confirm = vi
      .fn<ApplyDeps["confirmRestoreIssue"]>()
      .mockResolvedValueOnce("enable")
      .mockResolvedValueOnce(true);
    const d = deps({ kind: "disabled" }, {
      enableSystemRestore: vi.fn(async () => {
        throw "Access denied";
      }),
      confirmRestoreIssue: confirm,
    });
    await runApply(["ads"], [], d);
    expect(confirm).toHaveBeenLastCalledWith(
      { kind: "failed", message: "Couldn't turn on System Restore: Access denied" },
      { canEnable: false },
    );
    expect(d.applyTweaks).toHaveBeenCalledWith(["ads"], []);
  });
});

describe("appName", () => {
  it("strips the publisher prefix and spaces out the name", () => {
    expect(appName('Remove app "king.com.CandyCrushSaga" (all users)')).toBe("Candy Crush Saga");
    expect(appName('Remove app "Microsoft.Microsoft3DViewer"')).toBe("Microsoft 3D Viewer");
    expect(appName("HKLM\\Software\\X = 1 (DWORD)")).toBeNull();
  });
});

describe("groupApplyReport", () => {
  it("groups outcomes with titles and reasons", () => {
    const groups = groupApplyReport(
      {
        results: [
          { id: "ads", outcome: { kind: "applied" } },
          { id: "recall", outcome: { kind: "notApplicable", reason: "wrongBuild" } },
          { id: "junk", outcome: { kind: "failed", message: "Access denied" } },
        ],
      },
      tweaks,
    );
    expect(groups).toEqual([
      { label: "Applied", tone: "ok", items: [{ title: "Title ads" }] },
      { label: "Failed", tone: "bad", items: [{ title: "Title junk", detail: "Access denied" }] },
      {
        label: "Not applicable",
        tone: "muted",
        items: [{ title: "Title recall", detail: "Not available on this version of Windows" }],
      },
    ]);
  });
});

describe("groupUndoReport", () => {
  it("puts each tweak in its worst outcome group", () => {
    const groups = groupUndoReport(
      {
        results: [
          { id: "ads", outcomes: [{ kind: "restored" }, { kind: "alreadyOriginal" }] },
          { id: "recall", outcomes: [{ kind: "alreadyOriginal" }] },
          { id: "x", outcomes: [{ kind: "restored" }, { kind: "changedSince", what: "HKCU\A" }] },
          {
            id: "junk",
            outcomes: [
              { kind: "manualReinstall", app: "king.com.CandyCrushSaga", hint: "9NBLGGH18846" },
              { kind: "manualReinstall", app: "Microsoft.BingNews", hint: "9WZDNCRFHVFW" },
              { kind: "manualReinstall", app: "Clipchamp.Clipchamp", hint: null },
            ],
          },
          { id: "y", outcomes: [{ kind: "changedSince", what: "B" }, { kind: "failed", message: "Denied" }] },
        ],
      },
      tweaks,
    );
    expect(groups).toEqual([
      { label: "Restored", tone: "ok", items: [{ id: "ads", title: "Title ads" }] },
      { label: "Already original", tone: "muted", items: [{ id: "recall", title: "Title recall" }] },
      {
        label: "Changed since",
        tone: "bad",
        items: [{ id: "x", title: "x", detail: "Something else changed HKCU\A since Featherweight set it" }],
      },
      {
        label: "Reinstall manually",
        tone: "muted",
        items: [
          {
            id: "junk",
            title: "Candy Crush Saga",
            detail: "Reinstall it from the Microsoft Store (product ID 9NBLGGH18846)",
            storeId: "9NBLGGH18846",
          },
          {
            id: "junk",
            title: "Bing News",
            detail: "Reinstall it from the Microsoft Store (product ID 9WZDNCRFHVFW)",
            storeId: "9WZDNCRFHVFW",
          },
          { id: "junk", title: "Clipchamp", detail: "Reinstall it from the Microsoft Store" },
        ],
      },
      { label: "Failed", tone: "bad", items: [{ id: "y", title: "y", detail: "Denied" }] },
    ]);
  });

  it("lists a tweak's restored values and its removed apps separately", () => {
    const groups = groupUndoReport(
      {
        results: [
          {
            id: "junk",
            outcomes: [
              { kind: "restored" },
              { kind: "manualReinstall", app: "king.com.CandyCrushSaga", hint: "9NBLGGH18846" },
            ],
          },
        ],
      },
      tweaks,
    );
    expect(groups.map((g) => [g.label, g.items.map((i) => i.title)])).toEqual([
      ["Restored", ["Title junk"]],
      ["Reinstall manually", ["Candy Crush Saga"]],
    ]);
  });

  it("lists values journaled for another account on their own", () => {
    const groups = groupUndoReport(
      {
        results: [
          { id: "ads", outcomes: [{ kind: "otherAccount", what: "HKCU\\X" }] },
          { id: "junk", outcomes: [{ kind: "restored" }, { kind: "otherAccount", what: "HKCU\\Y" }] },
        ],
      },
      tweaks,
    );
    expect(groups).toEqual([
      { label: "Restored", tone: "ok", items: [{ id: "junk", title: "Title junk" }] },
      {
        label: "Belongs to another account",
        tone: "muted",
        items: [
          { id: "ads", title: "Title ads", detail: OTHER_ACCOUNT_DETAIL },
          { id: "junk", title: "Title junk", detail: OTHER_ACCOUNT_DETAIL },
        ],
      },
    ]);
    expect(resultHeadline(groups.slice(1)).title).toBe("These changes belong to another account");
  });

  it("builds a Store link only from a product id", () => {
    expect(storeLink("9NBLGGH18846")).toBe("ms-windows-store://pdp/?ProductId=9NBLGGH18846");
    expect(storeLink("bad id&x=1")).toBeNull();
  });
});

describe("mergeUndoReport", () => {
  it("replaces results by id and keeps order", () => {
    const merged = mergeUndoReport(
      {
        results: [
          { id: "a", outcomes: [{ kind: "changedSince", what: "X" }] },
          { id: "b", outcomes: [{ kind: "restored" }] },
        ],
      },
      { results: [{ id: "a", outcomes: [{ kind: "restored" }] }] },
    );
    expect(merged.results).toEqual([
      { id: "a", outcomes: [{ kind: "restored" }] },
      { id: "b", outcomes: [{ kind: "restored" }] },
    ]);
  });
});

describe("combineGroups and resultHeadline", () => {
  const g = (label: string, n = 1): ResultGroup => ({
    label,
    tone: "ok",
    items: Array.from({ length: n }, (_, i) => ({ title: `${label} ${i}` })),
  });

  it("renames clashing Failed groups", () => {
    expect(combineGroups([g("Applied"), g("Failed")], [g("Failed")]).map((x) => x.label)).toEqual([
      "Applied",
      "Failed to apply",
      "Failed to undo",
    ]);
    expect(combineGroups([g("Applied")], [g("Failed")]).map((x) => x.label)).toEqual(["Applied", "Failed"]);
  });

  it("says what happened", () => {
    expect(resultHeadline([g("Applied", 2)]).title).toBe("2 changes applied");
    expect(resultHeadline([g("Restored", 1)]).title).toBe("1 change put back");
    expect(resultHeadline([g("Applied", 1), g("Restored", 2)])).toEqual({
      title: "3 changes made",
      summary: "1 applied, 2 put back.",
    });
    expect(resultHeadline([g("Restored"), g("Failed")]).title).toBe("Some changes didn't go through");
    expect(resultHeadline([g("Restored"), g("Changed since")]).title).toBe("One setting was left alone");
    expect(resultHeadline([g("Changed since", 2), g("Failed")]).title).toBe("Some changes didn't go through");
    expect(resultHeadline([g("Already original")]).title).toBe("Nothing needed changing");
  });
});

describe("restorePointStep", () => {
  it("continues without asking when the restore point was made", async () => {
    const d = deps({ kind: "created" });
    expect(await restorePointStep(d)).toBe(true);
    expect(d.confirmRestoreIssue).not.toHaveBeenCalled();
  });

  it("returns false when the user cancels at a throttled restore point", async () => {
    const d = deps({ kind: "throttled" }, { confirmRestoreIssue: vi.fn().mockResolvedValue(false) });
    expect(await restorePointStep(d)).toBe(false);
  });
});
