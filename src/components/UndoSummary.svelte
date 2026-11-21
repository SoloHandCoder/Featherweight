<script lang="ts">
  // ResultSummary for anything that undid tweaks: undo groups (optionally
  // after apply groups), "Restore anyway" on Changed since rows and a Store
  // link on Reinstall manually rows. Pages call `show(...)` via bind:this.
  import { ExternalLink } from "@lucide/svelte";
  import ResultSummary from "./ResultSummary.svelte";
  import { backend } from "../lib/backend";
  import { app, changed, message, startBusy, stopBusy } from "../lib/state.svelte";
  import {
    combineGroups,
    groupUndoReport,
    mergeUndoReport,
    resultHeadline,
    storeLink,
    type ResultGroup,
    type ResultItem,
  } from "../lib/applyFlow";
  import type { UndoReport } from "../lib/types";

  interface Props {
    returnFocus?: HTMLElement | null;
  }

  let { returnFocus = null }: Props = $props();

  let open = $state(false);
  let applyGroups = $state<ResultGroup[]>([]);
  let report = $state<UndoReport>({ results: [] });
  /** Id whose "Restore anyway" is running. */
  let forcing = $state<string | null>(null);

  const groups = $derived(combineGroups(applyGroups, groupUndoReport(report, app.tweaks)));
  const headline = $derived(resultHeadline(groups));

  /** Opens the summary. `apply` are groups from groupApplyReport to list first. */
  export function show(undo: UndoReport | null, apply: ResultGroup[] = []) {
    report = undo ?? { results: [] };
    applyGroups = apply;
    open = true;
  }

  async function restoreAnyway(id: string) {
    if (app.busy) return;
    let forced: UndoReport | null = null;
    if (!startBusy({ kind: "undo", label: "Restoring" })) return;
    forcing = id;
    try {
      forced = await backend.api.undoTweaks([id], true);
      report = mergeUndoReport(report, forced);
    } catch (e) {
      report = mergeUndoReport(report, {
        results: [{ id, outcomes: [{ kind: "failed", message: `Couldn't restore: ${message(e)}` }] }],
      });
    } finally {
      forcing = null;
      stopBusy();
    }
    // Refreshes every open page: a section's switches and "changed since" notes.
    await changed(forced);
  }
</script>

<ResultSummary
  {open}
  title={headline.title}
  summary={headline.summary}
  {groups}
  onclose={() => (open = false)}
  {returnFocus}
>
  {#snippet itemAction(item: ResultItem, group: ResultGroup)}
    {#if group.label === "Changed since" && item.id}
      {@const id = item.id}
      <button
        type="button"
        class="btn btn-ghost btn-sm"
        aria-disabled={!!app.busy || undefined}
        data-busy={forcing === id || undefined}
        aria-label="Restore anyway: {item.title}"
        onclick={() => restoreAnyway(id)}
      >
        {forcing === id ? "Restoring" : "Restore anyway"}
      </button>
    {:else if item.storeId && storeLink(item.storeId)}
      <a class="btn btn-ghost btn-sm store" href={storeLink(item.storeId)} title={storeLink(item.storeId)}>
        <ExternalLink size={14} strokeWidth={1.75} aria-hidden="true" />
        Microsoft Store
      </a>
    {/if}
  {/snippet}
</ResultSummary>

<style>
  .btn {
    flex: none;
    white-space: nowrap;
  }

  .store {
    text-decoration: none;
  }
</style>
