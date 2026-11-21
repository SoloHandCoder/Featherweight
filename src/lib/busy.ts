// The one-change-at-a-time guard. Pure so it can be tested without Svelte;
// state.svelte.ts binds it to the shared `app` object.

export interface Busy {
  kind: "apply" | "undo" | "cleanup";
  /** Short present-tense text for the running change, e.g. "Applying Everyday". */
  label: string;
}

export interface BusyHolder {
  busy: Busy | null;
}

/** Marks `holder` busy. Returns false, changing nothing, when it already is. */
export function beginBusy(holder: BusyHolder, busy: Busy): boolean {
  if (holder.busy) return false;
  holder.busy = busy;
  return true;
}

export function endBusy(holder: BusyHolder): void {
  holder.busy = null;
}

export const BUSY_HINT = "Wait until the current change finishes";
