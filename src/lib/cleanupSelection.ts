import type { CleanupTarget } from "./types";

/** Targets whose id is in `selected`, in scan order. */
export function selectedTargets(targets: CleanupTarget[], selected: ReadonlySet<string>): CleanupTarget[] {
  return targets.filter((t) => selected.has(t.id));
}

/** Total bytes the selected targets would free. */
export function selectedBytes(targets: CleanupTarget[], selected: ReadonlySet<string>): number {
  return selectedTargets(targets, selected).reduce((sum, t) => sum + t.bytes, 0);
}
