import type { CleanupTarget } from "./types";

/** Targets whose id is in `selected`, in scan order. */
export function selectedTargets(targets: CleanupTarget[], selected: ReadonlySet<string>): CleanupTarget[] {
  return targets.filter((t) => selected.has(t.id));
}

/**
 * Selection after a (re)scan. Non-empty targets are checked by default;
 * a target the user had unchecked stays unchecked when it comes back.
 * Zero-byte targets are never checked.
 */
export function nextSelection(
  targets: CleanupTarget[],
  previousIds: ReadonlySet<string>,
  previousSelected: ReadonlySet<string>,
): Set<string> {
  const next = new Set<string>();
  for (const t of targets) {
    if (t.bytes <= 0) continue;
    if (!previousIds.has(t.id) || previousSelected.has(t.id)) next.add(t.id);
  }
  return next;
}

/** Total bytes the selected targets would free. */
export function selectedBytes(targets: CleanupTarget[], selected: ReadonlySet<string>): number {
  return selectedTargets(targets, selected).reduce((sum, t) => sum + t.bytes, 0);
}
