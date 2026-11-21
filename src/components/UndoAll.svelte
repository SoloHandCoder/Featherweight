<script lang="ts">
  // Sidebar "Undo all": confirm, undoAll(false), then the grouped result.
  // App calls `start()` via bind:this.
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import UndoSummary from "./UndoSummary.svelte";
  import { backend } from "../lib/backend";
  import { app, message, refresh, startBusy, stopBusy } from "../lib/state.svelte";

  let { returnFocus = null }: { returnFocus?: HTMLElement | null } = $props();

  let asking = $state(false);
  let summary: UndoSummary | undefined = $state();

  export function start() {
    if (app.busy || !app.backendAvailable) return;
    asking = true;
  }

  async function run() {
    asking = false;
    if (!startBusy({ kind: "undo", label: "Putting everything back" })) return;
    try {
      summary?.show(await backend.api.undoAll(false));
    } catch (e) {
      summary?.show({ results: [{ id: "Undo all", outcomes: [{ kind: "failed", message: message(e) }] }] });
    } finally {
      stopBusy();
    }
    await refresh();
  }
</script>

<ConfirmDialog
  open={asking}
  title="Put everything Featherweight changed back?"
  message="Every setting it changed goes back to how it was before. Removed apps have to be reinstalled from the Microsoft Store."
  confirmLabel="Undo all"
  cautious
  onconfirm={run}
  oncancel={() => (asking = false)}
  {returnFocus}
/>

<UndoSummary bind:this={summary} {returnFocus} />
