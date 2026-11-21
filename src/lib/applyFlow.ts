// The safe apply sequence shared by Home and the section pages:
// restore point -> (ask about restore point problems) -> (confirm
// irreversible tweaks) -> apply. UI questions are injected so the order
// is testable without a backend.

import type { ApplyReport, NaReason, RestorePointOutcome, TweakView, UndoReport } from "./types";

/** `true` continue, `false` cancel, `"enable"` turn on System Restore and retry. */
export type RestoreChoice = boolean | "enable";

export interface ApplyDeps {
  /** Used to tell irreversible tweaks apart. */
  tweaks: TweakView[];
  ensureRestorePoint: () => Promise<RestorePointOutcome>;
  enableSystemRestore: () => Promise<void>;
  applyTweaks: (ids: string[], allUsers: string[]) => Promise<ApplyReport>;
  /** Resolves true to go ahead with these irreversible ids, false to skip them. */
  confirmIrreversible: (ids: string[]) => Promise<boolean>;
  /** Asked when the restore point is Throttled, Disabled or Failed. */
  confirmRestoreIssue: (
    o: RestorePointOutcome,
    opts: { canEnable: boolean },
  ) => Promise<RestoreChoice>;
}

export function planApply(
  ids: string[],
  tweaks: TweakView[],
): { reversible: string[]; irreversible: string[] } {
  const irr = new Set(tweaks.filter((t) => t.irreversible).map((t) => t.id));
  return {
    reversible: ids.filter((id) => !irr.has(id)),
    irreversible: ids.filter((id) => irr.has(id)),
  };
}

function isIssue(o: RestorePointOutcome): boolean {
  return o.kind === "throttled" || o.kind === "disabled" || o.kind === "failed";
}

function message(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

/**
 * Makes the session's restore point, asking about problems. Returns false
 * when the user cancelled at the question. Shared with the startup toggles.
 */
export async function restorePointStep(
  deps: Pick<ApplyDeps, "ensureRestorePoint" | "enableSystemRestore" | "confirmRestoreIssue">,
): Promise<boolean> {
  let outcome = await deps.ensureRestorePoint();
  let triedEnable = false;
  while (isIssue(outcome)) {
    const canEnable = outcome.kind === "disabled" && !triedEnable;
    const choice = await deps.confirmRestoreIssue(outcome, { canEnable });
    if (choice === "enable" && canEnable) {
      triedEnable = true;
      try {
        await deps.enableSystemRestore();
        outcome = await deps.ensureRestorePoint();
      } catch (e) {
        outcome = { kind: "failed", message: `Couldn't turn on System Restore: ${message(e)}` };
      }
      continue;
    }
    return choice === true;
  }
  return true;
}

/**
 * Runs the full sequence. Resolves null when nothing was applied because
 * the user cancelled or skipped everything.
 */
export async function runApply(
  ids: string[],
  allUsers: string[],
  deps: ApplyDeps,
): Promise<ApplyReport | null> {
  if (ids.length === 0) return null;
  if (!(await restorePointStep(deps))) return null;

  const { irreversible } = planApply(ids, deps.tweaks);
  let chosen = ids;
  if (irreversible.length > 0 && !(await deps.confirmIrreversible(irreversible))) {
    const skip = new Set(irreversible);
    chosen = ids.filter((id) => !skip.has(id));
  }
  if (chosen.length === 0) return null;

  const keep = new Set(chosen);
  return deps.applyTweaks(
    chosen,
    allUsers.filter((id) => keep.has(id)),
  );
}

/**
 * Friendly app name from a `changes` line such as
 * `Remove app "king.com.CandyCrushSaga" (all users)` -> "Candy Crush Saga".
 * Null for lines that don't remove an app.
 */
export function appName(change: string): string | null {
  const m = /^Remove app "([^"]+)"/.exec(change);
  if (!m) return null;
  const base = m[1].split(".").pop() ?? m[1];
  return base
    .replace(/([a-z])([A-Z0-9])/g, "$1 $2")
    .replace(/([0-9A-Z])([A-Z][a-z])/g, "$1 $2");
}

export interface ResultItem {
  title: string;
  detail?: string;
  /** Tweak id, set on undo results so a row can offer "Restore anyway". */
  id?: string;
  /** Microsoft Store product id for "Reinstall manually" rows. */
  storeId?: string;
}

export interface ResultGroup {
  label: string;
  tone: "ok" | "muted" | "bad";
  items: ResultItem[];
}

export const NA_REASON: Record<NaReason, string> = {
  wrongBuild: "Not available on this version of Windows",
  accountMismatch: "Changes only the signed-in account, and Featherweight is running as another one",
  missingOnThisPc: "Not on this PC",
};

/** Apply outcomes grouped for ResultSummary; empty groups are left out. */
export function groupApplyReport(report: ApplyReport, tweaks: TweakView[]): ResultGroup[] {
  const title = (id: string) => tweaks.find((t) => t.id === id)?.title ?? id;
  const applied: ResultItem[] = [];
  const already: ResultItem[] = [];
  const failed: ResultItem[] = [];
  const na: ResultItem[] = [];
  for (const r of report.results) {
    const o = r.outcome;
    if (o.kind === "applied") applied.push({ title: title(r.id) });
    else if (o.kind === "alreadyApplied") already.push({ title: title(r.id) });
    else if (o.kind === "failed") failed.push({ title: title(r.id), detail: o.message });
    else na.push({ title: title(r.id), detail: NA_REASON[o.reason] });
  }
  const groups: ResultGroup[] = [
    { label: "Applied", tone: "ok", items: applied },
    { label: "Already applied", tone: "muted", items: already },
    { label: "Failed", tone: "bad", items: failed },
    { label: "Not applicable", tone: "muted", items: na },
  ];
  return groups.filter((g) => g.items.length > 0);
}

/** `ms-windows-store://` link for a Store product id; null for anything else. */
export function storeLink(productId: string): string | null {
  return /^[A-Za-z0-9]+$/.test(productId) ? `ms-windows-store://pdp/?ProductId=${productId}` : null;
}

/**
 * Undo outcomes grouped for ResultSummary, one row per tweak in its most
 * serious group: Failed > Changed since > Reinstall manually > Restored >
 * Already original. Empty groups are left out.
 */
export function groupUndoReport(report: UndoReport, tweaks: TweakView[]): ResultGroup[] {
  const title = (id: string) => tweaks.find((t) => t.id === id)?.title ?? id;
  const restored: ResultItem[] = [];
  const original: ResultItem[] = [];
  const changed: ResultItem[] = [];
  const manual: ResultItem[] = [];
  const failed: ResultItem[] = [];
  for (const r of report.results) {
    const item: ResultItem = { id: r.id, title: title(r.id) };
    const fail = r.outcomes.find((o) => o.kind === "failed");
    const since = r.outcomes.find((o) => o.kind === "changedSince");
    const reinstall = r.outcomes.find((o) => o.kind === "manualReinstall");
    if (fail) {
      failed.push({ ...item, detail: fail.message });
    } else if (since) {
      changed.push({ ...item, detail: `Something else changed ${since.what} since Featherweight set it` });
    } else if (reinstall) {
      manual.push({
        ...item,
        ...(reinstall.hint
          ? { detail: `Reinstall it from the Microsoft Store (product ID ${reinstall.hint})`, storeId: reinstall.hint }
          : { detail: "Reinstall it from the Microsoft Store" }),
      });
    } else if (r.outcomes.some((o) => o.kind === "restored")) {
      restored.push(item);
    } else {
      original.push(item);
    }
  }
  const groups: ResultGroup[] = [
    { label: "Restored", tone: "ok", items: restored },
    { label: "Already original", tone: "muted", items: original },
    { label: "Changed since", tone: "bad", items: changed },
    { label: "Reinstall manually", tone: "muted", items: manual },
    { label: "Failed", tone: "bad", items: failed },
  ];
  return groups.filter((g) => g.items.length > 0);
}

/** `base` with the results for the ids in `update` replaced by the new ones. */
export function mergeUndoReport(base: UndoReport, update: UndoReport): UndoReport {
  const next = new Map(update.results.map((r) => [r.id, r]));
  const results = base.results.map((r) => next.get(r.id) ?? r);
  for (const r of update.results) if (!base.results.some((b) => b.id === r.id)) results.push(r);
  return { results };
}

/** Heading and one-line summary for a result that may mix applies and undos. */
export function resultHeadline(groups: ResultGroup[]): { title: string; summary?: string } {
  const count = (...labels: string[]) =>
    groups.filter((g) => labels.includes(g.label)).reduce((n, g) => n + g.items.length, 0);
  const applied = count("Applied");
  const restored = count("Restored");
  const problems = count("Failed", "Failed to apply", "Failed to undo", "Changed since");
  const plural = (n: number) => `${n} ${n === 1 ? "change" : "changes"}`;
  if (problems > 0) return { title: "Some changes didn't go through", summary: "The ones that didn't say why." };
  if (applied + restored === 0) {
    if (count("Reinstall manually") > 0) return { title: "Some apps need reinstalling" };
    return { title: "Nothing needed changing", summary: "This PC already had these settings." };
  }
  if (restored === 0) return { title: `${plural(applied)} applied` };
  if (applied === 0) return { title: `${plural(restored)} put back` };
  return { title: `${plural(applied + restored)} made`, summary: `${applied} applied, ${restored} put back.` };
}

/**
 * Apply groups followed by undo groups. When both have a Failed group
 * they become "Failed to apply" and "Failed to undo".
 */
export function combineGroups(apply: ResultGroup[], undo: ResultGroup[]): ResultGroup[] {
  const clash = apply.some((g) => g.label === "Failed") && undo.some((g) => g.label === "Failed");
  if (!clash) return [...apply, ...undo];
  const rename = (label: string) => (g: ResultGroup) => (g.label === "Failed" ? { ...g, label } : g);
  return [...apply.map(rename("Failed to apply")), ...undo.map(rename("Failed to undo"))];
}
