// The command layer the UI talks to. Normally the real Tauri commands; in a
// dev browser opened with `?mock`, state.svelte.ts swaps in src/lib/mock.ts.

import * as real from "./api";

export type Backend = Omit<typeof real, "APPLY_PROGRESS">;

export const backend: { api: Backend } = { api: real };
