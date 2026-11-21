// The safe apply sequence shared by Home and the section pages:
// restore point -> (ask about restore point problems) -> (confirm
// irreversible tweaks) -> apply. UI questions are injected so the order
// is testable without a backend.

import type {
  ApplyReport,
  CleanupReport,
  CleanupTarget,
  NaReason,
  RestorePointOutcome,
  TweakView,
  UndoReport,
} from "./types";

/** `true` continue, `false` cancel, `"enable"` turn on System Restore and retry. */
export type RestoreChoice = boolean | "enable";

export interface ApplyDeps {
  /** Used to tell irreversible tweaks apart. */
  tweaks: TweakView[];
  ensureRestorePoint: () => Promise<RestorePointOutcome>;
  enableSystemRestore: () => Promise<void>;
  applyTweaks: (ids: string[], allUsers: string[]) => Promise<ApplyReport>;
  /**
   * Resolves true to go ahead with these irreversible ids (and, for a
   * preset, these cleanup targets), false to skip all of them.
   */
  confirmIrreversible: (ids: string[], cleanup?: CleanupTarget[]) => Promise<boolean>;
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

export interface PresetDeps extends ApplyDeps {
  scanCleanup: () => Promise<CleanupTarget[]>;
  runCleanup: (ids: string[]) => Promise<CleanupReport>;
}

export interface PresetResult {
  /** Null when no tweak was applied (none picked, or all skipped). */
  apply: ApplyReport | null;
  /** Null when no cleanup ran. */
  cleanup: CleanupReport | null;
  /** Set when the scan or the cleanup itself failed; the tweaks still ran. */
  cleanupError: string | null;
}

/**
 * A preset: its tweaks plus every cleanup target with something in it.
 * restore point -> scan -> one confirmation for the irreversible tweaks
 * and the cleanup together ("Skip these" skips both) -> apply -> clean.
 * Resolves null when the user cancelled at the restore point question.
 */
export async function runPreset(ids: string[], deps: PresetDeps): Promise<PresetResult | null> {
  if (!(await restorePointStep(deps))) return null;

  let targets: CleanupTarget[] = [];
  let cleanupError: string | null = null;
  try {
    targets = (await deps.scanCleanup()).filter((t) => t.bytes > 0);
  } catch (e) {
    cleanupError = `Couldn't look for junk files: ${message(e)}`;
  }

  const { irreversible } = planApply(ids, deps.tweaks);
  let chosen = ids;
  if (irreversible.length > 0 || targets.length > 0) {
    if (!(await deps.confirmIrreversible(irreversible, targets))) {
      const skip = new Set(irreversible);
      chosen = ids.filter((id) => !skip.has(id));
      targets = [];
    }
  }

  const apply = chosen.length > 0 ? await deps.applyTweaks(chosen, []) : null;
  let cleanup: CleanupReport | null = null;
  if (targets.length > 0) {
    try {
      cleanup = await deps.runCleanup(targets.map((t) => t.id));
    } catch (e) {
      cleanupError = `Couldn't clean up: ${message(e)}`;
    }
  }
  return { apply, cleanup, cleanupError };
}

/**
 * Friendly app name from a `changes` line such as
 * `Remove app "king.com.CandyCrushSaga" (all users)` -> "Candy Crush Saga".
 * Null for lines that don't remove an app.
 */
export function appName(change: string): string | null {
  const m = /^Remove app "([^"]+)"/.exec(change);
  return m ? packageTitle(m[1]) : null;
}

/** Friendly name of an Appx package: "king.com.CandyCrushSaga" -> "Candy Crush Saga". */
export function packageTitle(pkg: string): string {
  const base = pkg.split(".").pop() || pkg;
  return base
    .replace(/([a-z])([A-Z0-9])/g, "$1 $2")
    .replace(/([0-9A-Z])([A-Z][a-z])/g, "$1 $2");
}

/** Tweaks whose way back isn't the Store, by id. */
const IRREVERSIBLE_NOTES: Record<string, string> = {
  "remove-onedrive": "Undo reinstalls OneDrive. You sign in to it again afterwards.",
};

/** What getting an irreversible tweak's change back takes, for its confirmation row. */
export function irreversibleNote(t: TweakView): string {
  const note = IRREVERSIBLE_NOTES[t.id];
  if (note) return note;
  if (t.changes.some((c) => appName(c) !== null)) return "You can reinstall these apps from the Microsoft Store.";
  return "Undo may not put this back exactly as it was.";
}

/** The note shown while an applied irreversible tweak's switch is turned off. */
export function turnOffNote(t: TweakView): string {
  return (
    IRREVERSIBLE_NOTES[t.id] ??
    "Turning this off won't bring removed apps back — reinstall them from the Microsoft Store."
  );
}

/** Message and button of the "can't be undone" question: app removals, files, or both. */
export function irreversibleCopy(tweaks: boolean, files: boolean): { message: string; confirm: string } {
  if (tweaks && files)
    return { message: "Undo can't fully put these back, and deleted files are gone.", confirm: "Remove and clean" };
  if (files) return { message: "Undo can't bring deleted files back.", confirm: "Clean" };
  return { message: "Undo can't fully put these back.", confirm: "Remove" };
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
  /** A one-line group (e.g. "Cleaned up") reads better without the item count. */
  hideCount?: boolean;
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

export const OTHER_ACCOUNT_DETAIL =
  "Featherweight changed this for another Windows account. Run it as that account to undo it.";

/**
 * Undo outcomes grouped for ResultSummary. Each tweak gets one row in its
 * most serious group (Failed > Changed since > Restored > Already original)
 * for the values it put back, and every app it had removed gets its own
 * "Reinstall manually" row with that app's Store product id. Empty groups
 * are left out.
 */
export function groupUndoReport(report: UndoReport, tweaks: TweakView[]): ResultGroup[] {
  const title = (id: string) => tweaks.find((t) => t.id === id)?.title ?? id;
  const restored: ResultItem[] = [];
  const original: ResultItem[] = [];
  const changed: ResultItem[] = [];
  const manual: ResultItem[] = [];
  const failed: ResultItem[] = [];
  const other: ResultItem[] = [];
  for (const r of report.results) {
    const item: ResultItem = { id: r.id, title: title(r.id) };
    const rest = r.outcomes.filter((o) => o.kind !== "manualReinstall" && o.kind !== "otherAccount");
    if (r.outcomes.some((o) => o.kind === "otherAccount")) {
      other.push({ ...item, detail: OTHER_ACCOUNT_DETAIL });
    }
    for (const o of r.outcomes) {
      if (o.kind !== "manualReinstall") continue;
      manual.push({
        id: r.id,
        title: packageTitle(o.app),
        ...(o.hint
          ? { detail: `Reinstall it from the Microsoft Store (product ID ${o.hint})`, storeId: o.hint }
          : { detail: "Reinstall it from the Microsoft Store" }),
      });
    }
    const fail = rest.find((o) => o.kind === "failed");
    const since = rest.find((o) => o.kind === "changedSince");
    if (fail) {
      failed.push({ ...item, detail: fail.message });
    } else if (since) {
      changed.push({ ...item, detail: `Something else changed ${since.what} since Featherweight set it` });
    } else if (rest.some((o) => o.kind === "restored")) {
      restored.push(item);
    } else if (rest.length > 0 || r.outcomes.length === 0) {
      original.push(item);
    }
  }
  const groups: ResultGroup[] = [
    { label: "Restored", tone: "ok", items: restored },
    { label: "Already original", tone: "muted", items: original },
    { label: "Changed since", tone: "bad", items: changed },
    { label: "Reinstall manually", tone: "muted", items: manual },
    { label: "Belongs to another account", tone: "muted", items: other },
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
  const failures = count("Failed", "Failed to apply", "Failed to undo");
  const changedSince = count("Changed since");
  const plural = (n: number) => `${n} ${n === 1 ? "change" : "changes"}`;
  if (failures > 0) return { title: "Some changes didn't go through", summary: "Each one that didn't says why." };
  if (changedSince > 0)
    return {
      ...(changedSince === 1
        ? {
            title: "One setting was left alone",
            summary: "Something else changed it after Featherweight did. Restore anyway puts it back.",
          }
        : {
            title: `${changedSince} settings were left alone`,
            summary: "Something else changed them after Featherweight did. Restore anyway puts them back.",
          }),
    };
  if (applied + restored === 0) {
    if (count("Reinstall manually") > 0) return { title: "Some apps need reinstalling" };
    if (count("Belongs to another account") > 0) return { title: "These changes belong to another account" };
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
