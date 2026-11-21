// Shared app state. Svelte 5 can't export a reassignable `$state` binding
// from a module, so everything lives on one reactive object: read and
// write `app.page`, `app.tweaks`, `app.overview` from any component.

import { isTauri } from "@tauri-apps/api/core";
import * as api from "./api";
import type { Overview, TweakView } from "./types";

export type Page = "home" | "remove" | "background" | "gaming" | "cleanup";

export const app = $state({
  page: "home" as Page,
  tweaks: [] as TweakView[],
  overview: null as Overview | null,
  /** Last refresh error, shown in an inline banner. Null when fine. */
  error: null as string | null,
  /** False in a plain browser (`npm run dev` without Tauri). */
  backendAvailable: isTauri(),
  loading: false,
});

export const BACKEND_UNAVAILABLE =
  "Can't reach the Featherweight backend. Open the desktop app to see and change this PC's settings.";

function message(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

/** Reloads the overview and the tweak list; failures land in `app.error`. */
export async function refresh(): Promise<void> {
  if (!isTauri()) {
    app.backendAvailable = false;
    app.error = BACKEND_UNAVAILABLE;
    return;
  }
  app.backendAvailable = true;
  app.loading = true;
  try {
    const [overview, tweaks] = await Promise.all([api.getOverview(), api.listTweaks()]);
    app.overview = overview;
    app.tweaks = tweaks;
    app.error = null;
  } catch (e) {
    app.error = message(e);
  } finally {
    app.loading = false;
  }
}

export function navigate(page: Page): void {
  app.page = page;
}
