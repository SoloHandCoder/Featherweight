import { describe, expect, it, vi } from "vitest";
import { appName, groupApplyReport, planApply, runApply, type ApplyDeps } from "./applyFlow";
import type { ApplyReport, RestorePointOutcome, TweakView } from "./types";

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
