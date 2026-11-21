// Shared app state. Svelte 5 can't export a reassignable `$state` binding
// from a module, so everything lives on one reactive object: read and
// write `app.page`, `app.tweaks`, `app.overview` from any component.

import { isTauri } from "@tauri-apps/api/core";
import { backend } from "./backend";
import type { Overview, Preset, TweakView } from "./types";

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
  /** Ids from `previewPreset`: the preset's Safe tweaks that apply on this PC. */
  presets: { everyday: [], gaming: [] } as Record<Preset, string[]>,
  /** Last refresh error, shown in an inline banner. Null when fine. */
  error: null as string | null,
  /** False in a plain browser (`npm run dev` without Tauri). */
  backendAvailable: isTauri() || useMock,
  loading: false,
});

export const BACKEND_UNAVAILABLE =
  "Can't reach the Featherweight backend. Open the desktop app to see and change this PC's settings.";

export function message(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

/** Reloads the overview, the tweak list and the preset previews; failures land in `app.error`. */
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
    const [overview, tweaks, everyday, gaming] = await Promise.all([
      api.getOverview(),
      api.listTweaks(),
      api.previewPreset("everyday"),
      api.previewPreset("gaming"),
    ]);
    app.overview = overview;
    app.tweaks = tweaks;
    app.presets = { everyday, gaming };
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
