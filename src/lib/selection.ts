// Section page switches: what each switch starts as, and what applying the
// switches would change. A switch shows the wanted end state; only the
// difference from the PC's current state is sent to the backend.

import type { Section, TweakView } from "./types";

export type Toggles = Record<string, boolean>;

function isOn(t: TweakView): boolean {
  return t.status.kind === "applied" || t.status.kind === "partial";
}

/** Applied/Partial start on, everything else off (so Advanced is never pre-selected). */
export function defaultToggles(tweaks: TweakView[]): Toggles {
  return Object.fromEntries(tweaks.map((t) => [t.id, isOn(t)]));
}

/**
 * On and not fully Applied -> apply (a Partial one is applied again to
 * finish it). Off and Applied/Partial -> undo. Tweaks with no entry in
 * `toggled` and NotApplicable tweaks are left alone.
 */
export function pendingChanges(tweaks: TweakView[], toggled: Toggles): { apply: string[]; undo: string[] } {
  const apply: string[] = [];
  const undo: string[] = [];
  for (const t of tweaks) {
    const want = toggled[t.id];
    if (want === undefined || t.status.kind === "notApplicable") continue;
    if (want && t.status.kind !== "applied") apply.push(t.id);
    else if (!want && isOn(t)) undo.push(t.id);
  }
  return { apply, undo };
}

/**
 * What an exclusive switch displaced: switch id -> the partner's wanted
 * state just before that switch turned it off.
 */
export type Displaced = Record<string, boolean>;

/**
 * Sets one switch. Switching one on switches off its `exclusiveWith`
 * partner and remembers what the partner was; switching it back off
 * (before applying) puts the partner back. Touching the partner directly
 * forgets the memory.
 */
export function switchToggle(
  tweaks: TweakView[],
  toggled: Toggles,
  displaced: Displaced,
  id: string,
  on: boolean,
): { toggles: Toggles; displaced: Displaced } {
  const next = { ...toggled, [id]: on };
  const memo = { ...displaced };
  const partner = tweaks.find((t) => t.id === id)?.exclusiveWith ?? null;
  // The user set `id` by hand, so whatever `partner` displaced in it is moot.
  if (partner) delete memo[partner];
  if (partner && partner in next) {
    if (on) {
      if (!(id in memo)) memo[id] = toggled[partner];
      next[partner] = false;
    } else if (id in memo) {
      next[partner] = memo[id];
      delete memo[id];
    }
  }
  if (!on) delete memo[id];
  return { toggles: next, displaced: memo };
}

/** `switchToggle` without the memory: switching one on switches off its partner. */
export function setToggle(tweaks: TweakView[], toggled: Toggles, id: string, on: boolean): Toggles {
  return switchToggle(tweaks, toggled, {}, id, on).toggles;
}

/** The section's tweaks that can run on this PC, in catalog order. */
export function visibleTweaks(tweaks: TweakView[], section: Section): TweakView[] {
  return tweaks.filter((t) => t.section === section && t.status.kind !== "notApplicable");
}
