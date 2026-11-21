// Shared app state. Svelte 5 can't export a reassignable `$state` binding
// from a module, so everything lives on one reactive object: read and
// write `app.page`, `app.tweaks`, `app.overview` from any component.

import { isTauri } from "@tauri-apps/api/core";
import { backend } from "./backend";
import { beginBusy, endBusy, type Busy } from "./busy";
import { dialogClosed, dialogOpened, recordChange } from "./counters";
import { presetIds } from "./selection";
import type { Overview, Preset, TweakView, UndoReport } from "./types";

export type Page = "home" | "remove" | "background" | "gaming" | "cleanup";

/**
 * Dev only: `npm run dev` in a plain browser with `?mock` in the URL uses
 * the fake backend in mock.ts. Always false in a production build.
 */
const useMock =
  import.meta.env.DEV &&
  !isTauri() &&
  typeof location !== "undefined" &&
  new URLSearchParams(location.search).has("mock");

export const app = $state({
  page: "home" as Page,
  tweaks: [] as TweakView[],
  overview: null as Overview | null,
  /** The preset's Safe tweaks that apply on this PC (from `listTweaks`). */
  presets: { everyday: [], gaming: [] } as Record<Preset, string[]>,
  /** Last refresh error, shown in an inline banner. Null when fine. */
  error: null as string | null,
  /** False in a plain browser (`npm run dev` without Tauri). */
  backendAvailable: isTauri() || useMock,
  loading: false,
  /**
   * The change running right now (apply, undo or cleanup), or null. Only one
   * runs at a time: start with `startBusy`, finish with `stopBusy` in a
   * `finally`. While set, the sidebar can't navigate or Undo all.
   */
  busy: null as Busy | null,
  /**
   * Bumped by `changed()` after every apply or undo, wherever it ran. An
   * open section page resets its switches when this moves.
   */
  revision: 0,
  /** Value name per tweak whose last undo found it changed by something else. */
  changedSince: {} as Record<string, string>,
  /** Open dialogs. The app shell is `inert` while this is above zero. */
  dialogs: 0,
});

export type { Busy } from "./busy";

/**
 * Claims the busy slot. Returns false when another change is running, and
 * the caller must then do nothing:
 *
 *   if (!startBusy({ kind: "apply", label: "Applying Everyday" })) return;
 *   try { ... } finally { stopBusy(); }
 */
export function startBusy(busy: Busy): boolean {
  return beginBusy(app, busy);
}

export function stopBusy(): void {
  endBusy(app);
}

export const BACKEND_UNAVAILABLE =
  "Can't reach the Featherweight backend. Open the desktop app to see and change this PC's settings.";

export function message(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

/**
 * Reloads the overview and the tweak list (which also says which tweaks each
 * preset holds: one status pass on the backend); failures land in `app.error`.
 */
export async function refresh(): Promise<void> {
  if (import.meta.env.DEV && useMock) {
    backend.api = (await import("./mock")).mockApi;
  } else if (!isTauri()) {
    app.backendAvailable = false;
    app.error = BACKEND_UNAVAILABLE;
    return;
  }
  app.backendAvailable = true;
  app.loading = true;
  try {
    const api = backend.api;
    const [overview, tweaks] = await Promise.all([api.getOverview(), api.listTweaks()]);
    app.overview = overview;
    app.tweaks = tweaks;
    app.presets = { everyday: presetIds(tweaks, "everyday"), gaming: presetIds(tweaks, "gaming") };
    app.error = null;
  } catch (e) {
    app.error = message(e);
  } finally {
    app.loading = false;
  }
}

/**
 * Call after an apply or undo finished (successfully or not): reloads,
 * then notes the undo report's "changed since" values and bumps
 * `app.revision`, so every open page shows the PC as it is now.
 */
export async function changed(undo: UndoReport | null = null): Promise<void> {
  await refresh();
  recordChange(app, undo);
}

export function openDialog(): void {
  dialogOpened(app);
}

export function closeDialog(): void {
  dialogClosed(app);
}

/** Ignored while a change is running. */
export function navigate(page: Page): void {
  if (app.busy) return;
  app.page = page;
}
