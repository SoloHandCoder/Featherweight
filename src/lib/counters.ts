// Shared counters on the app state, kept pure so they can be tested
// without Svelte. state.svelte.ts binds them to the shared `app` object.

import type { UndoReport } from "./types";

export interface ChangeHolder {
  /** Bumped after every apply or undo; open section pages reset their switches when it changes. */
  revision: number;
  /** Value name per tweak whose last undo found it changed by something else. */
  changedSince: Record<string, string>;
}

export interface DialogHolder {
  /** Dialogs open right now. While above zero the page behind them is inert. */
  dialogs: number;
}

/**
 * `map` updated from an undo report: a tweak whose undo found a value
 * changed by something else is noted; any other tweak in the report is
 * cleared (it was put back, or failed for another reason).
 */
export function mergeChangedSince(map: Record<string, string>, report: UndoReport): Record<string, string> {
  const next = { ...map };
  for (const r of report.results) {
    const o = r.outcomes.find((x) => x.kind === "changedSince");
    if (o) next[r.id] = o.what;
    else delete next[r.id];
  }
  return next;
}

/** Records that tweaks changed: notes the undo report (if any) and bumps the revision. */
export function recordChange(holder: ChangeHolder, undo: UndoReport | null): void {
  if (undo) holder.changedSince = mergeChangedSince(holder.changedSince, undo);
  holder.revision += 1;
}

export function dialogOpened(holder: DialogHolder): void {
  holder.dialogs += 1;
}

/** Never goes below zero, so a stray close can't leave the page inert later. */
export function dialogClosed(holder: DialogHolder): void {
  holder.dialogs = Math.max(0, holder.dialogs - 1);
}
