<script lang="ts">
  // A section's tweaks as switches, with an explainer for the selected one.
  // The switches show the wanted end state; "Apply N changes" applies the
  // ones switched on (through the safe apply flow) and undoes the ones
  // switched off. Used for Remove and Gaming; Background composes it.
  import { LifeBuoy } from "lucide-svelte";
  import TweakRow from "../components/TweakRow.svelte";
  import ExplainerPanel from "../components/ExplainerPanel.svelte";
  import LevelPill from "../components/LevelPill.svelte";
  import ApplyDialogs from "../components/ApplyDialogs.svelte";
  import UndoSummary from "../components/UndoSummary.svelte";
  import { backend } from "../lib/backend";
  import { app, message, refresh, startBusy, stopBusy } from "../lib/state.svelte";
  import { groupApplyReport, runApply, type ResultGroup } from "../lib/applyFlow";
  import { defaultToggles, pendingChanges, setToggle, visibleTweaks, type Toggles } from "../lib/selection";
  import type { Progress, UndoReport } from "../lib/types";

  let { section }: { section: "remove" | "background" | "gaming" } = $props();

  const uid = $props.id();
  const explainerId = `${uid}-explainer`;

  /** Switches the user moved; everything else shows the PC's state. */
  let overrides = $state<Toggles>({});
  let selectedId = $state<string | null>(null);
  /** Ids whose optional app removal should cover every account. */
  let allUsers = $state<string[]>([]);
  /** Value name per tweak whose last undo found it changed by something else. */
  let changedSince = $state<Record<string, string>>({});

  let phase = $state<"restore" | "apply" | "undo" | null>(null);
  let progress = $state<Progress | null>(null);
  let error = $state<string | null>(null);
  let nothingChanged = $state(false);

  let list: HTMLUListElement | undefined = $state();
  let applyButton: HTMLButtonElement | null = $state(null);
  let dialogs: ApplyDialogs | undefined = $state();
  let summary: UndoSummary | undefined = $state();

  const visible = $derived(visibleTweaks(app.tweaks, section));
  const toggles = $derived({ ...defaultToggles(visible), ...overrides });
  const pending = $derived(pendingChanges(visible, toggles));
  const count = $derived(pending.apply.length + pending.undo.length);
  const onCount = $derived(visible.filter((t) => toggles[t.id]).length);
  const selected = $derived(visible.find((t) => t.id === selectedId) ?? null);
  const busy = $derived(app.busy !== null);
  const canAct = $derived(app.backendAvailable && app.overview !== null && !app.overview.journalError);

  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

  function toggle(id: string, on: boolean) {
    if (busy) return;
    overrides = setToggle(visible, toggles, id, on);
    nothingChanged = false;
  }

  function onRowKey(e: KeyboardEvent) {
    if (!list || !["ArrowUp", "ArrowDown", "Home", "End"].includes(e.key)) return;
    const rows = Array.from(list.querySelectorAll<HTMLButtonElement>("[data-row]"));
    const i = rows.indexOf(e.currentTarget as HTMLButtonElement);
    let next = i;
    if (e.key === "ArrowDown") next = Math.min(i + 1, rows.length - 1);
    else if (e.key === "ArrowUp") next = Math.max(i - 1, 0);
    else if (e.key === "Home") next = 0;
    else next = rows.length - 1;
    e.preventDefault();
    rows[next]?.focus();
    selectedId = rows[next]?.dataset.row ?? selectedId;
  }

  /** Remembers which values something else changed since they were set. */
  function noteChangedSince(report: UndoReport) {
    const next = { ...changedSince };
    for (const r of report.results) {
      const o = r.outcomes.find((x) => x.kind === "changedSince");
      if (o) next[r.id] = o.what;
      else delete next[r.id];
    }
    changedSince = next;
  }

  /** `reset`: something ran, so the switches go back to showing the PC. */
  async function finish(reset = true) {
    if (reset) overrides = {};
    await refresh();
  }

  async function apply() {
    if (!canAct || count === 0) return;
    const { apply: applyIds, undo: undoIds } = pending;
    if (!startBusy({ kind: "apply", label: `Applying ${plural(applyIds.length + undoIds.length, "change", "changes")}` }))
      return;
    error = null;
    nothingChanged = false;
    let cancelled = false;
    let ran = true;
    let unlisten: (() => void) | undefined;
    const api = backend.api;
    // Undos run before applies: switching to an exclusive partner (e.g. one
    // power plan for another) must put the old one back first, or undoing it
    // afterwards would overwrite the new one. They still wait for the
    // restore point and confirmation steps.
    let undoReport: UndoReport | null = null;
    const runUndo = async () => {
      if (undoIds.length === 0 || undoReport) return;
      phase = "undo";
      progress = null;
      undoReport = await api.undoTweaks(undoIds, false);
      noteChangedSince(undoReport);
    };
    try {
      let applyGroups: ResultGroup[] = [];
      if (applyIds.length > 0) {
        const report = await runApply(
          applyIds,
          allUsers.filter((id) => applyIds.includes(id)),
          {
            tweaks: app.tweaks,
            ensureRestorePoint: () => {
              phase = "restore";
              return api.ensureRestorePoint();
            },
            enableSystemRestore: () => api.enableSystemRestore(),
            confirmRestoreIssue: async (outcome, { canEnable }) => {
              phase = null;
              const choice = await dialogs!.askRestore(outcome, canEnable);
              if (choice === false) cancelled = true;
              return choice;
            },
            confirmIrreversible: (ids) => {
              phase = null;
              return dialogs!.askIrreversible(ids);
            },
            applyTweaks: async (ids, au) => {
              await runUndo();
              phase = "apply";
              progress = { done: 0, total: ids.length, current: "" };
              unlisten = await api.onApplyProgress((p) => (progress = p));
              return api.applyTweaks(ids, au);
            },
          },
        );
        if (report) applyGroups = groupApplyReport(report, app.tweaks);
      }
      // Cancelling at the restore point question stops everything.
      if (!cancelled) await runUndo();
      if (applyGroups.length > 0 || undoReport) summary?.show(undoReport, applyGroups);
      else {
        nothingChanged = true;
        ran = false;
      }
    } catch (e) {
      error = `Couldn't apply: ${message(e)}`;
    } finally {
      unlisten?.();
      stopBusy();
      phase = null;
      progress = null;
    }
    await finish(ran);
  }

  async function undoOne(id: string, force: boolean) {
    if (!canAct) return;
    if (!startBusy({ kind: "undo", label: force ? "Restoring" : "Undoing" })) return;
    error = null;
    phase = "undo";
    try {
      const report = await backend.api.undoTweaks([id], force);
      noteChangedSince(report);
      summary?.show(report);
    } catch (e) {
      error = `Couldn't undo: ${message(e)}`;
    } finally {
      stopBusy();
      phase = null;
    }
    await finish();
  }
</script>

<div class="section">
  <div class="list-pane">
    <header class="list-head">
      <span class="muted num">
        {plural(visible.length, "item", "items")} · {onCount} selected
      </span>
      <span class="legend" aria-hidden="true">
        <LevelPill level="safe" />
        <LevelPill level="advanced" />
      </span>
    </header>

    {#if visible.length === 0}
      <p class="empty muted">{app.overview ? "Nothing in this section applies to this PC." : "Loading"}</p>
    {:else}
      <ul bind:this={list} aria-label="Changes">
        {#each visible as t (t.id)}
          <TweakRow
            tweak={t}
            on={toggles[t.id]}
            selected={selectedId === t.id}
            disabled={busy || !canAct}
            controls={explainerId}
            ontoggle={(on) => toggle(t.id, on)}
            onselect={() => (selectedId = t.id)}
            onkeydown={onRowKey}
          />
        {/each}
      </ul>
    {/if}

    <footer class="list-foot">
      <div class="note" aria-live="polite">
        {#if phase === "restore"}
          <p>Making a restore point</p>
        {:else if phase === "apply" && progress}
          <div class="progress">
            <p class="num">
              {#if progress.done >= progress.total}
                Finishing up
              {:else}
                Applying {progress.done + 1} of {progress.total}{progress.current ? ` — ${progress.current}` : ""}
              {/if}
            </p>
            <div class="bar" aria-hidden="true">
              <div class="fill" style:width="{progress.total ? (progress.done / progress.total) * 100 : 0}%"></div>
            </div>
          </div>
        {:else if phase === "undo"}
          <p>Putting changes back</p>
        {:else if error}
          <p class="error" role="alert">{error}</p>
        {:else if nothingChanged}
          <p>Nothing was changed.</p>
        {:else}
          <LifeBuoy size={14} strokeWidth={1.75} aria-hidden="true" />
          <p>A restore point is made before the first change.</p>
        {/if}
      </div>
      <button
        bind:this={applyButton}
        type="button"
        class="btn btn-primary num"
        disabled={!busy && (!canAct || count === 0)}
        aria-disabled={busy || undefined}
        onclick={apply}
      >
        {busy ? (app.busy?.label ?? "Working") : count === 0 ? "No changes to apply" : `Apply ${plural(count, "change", "changes")}`}
      </button>
    </footer>
  </div>

  <ExplainerPanel
    id={explainerId}
    tweak={selected}
    allUsers={!!selected && allUsers.includes(selected.id)}
    onallusers={(on) => {
      const id = selected?.id;
      if (!id) return;
      allUsers = on ? [...allUsers, id] : allUsers.filter((x) => x !== id);
    }}
    changedSince={selected ? (changedSince[selected.id] ?? null) : null}
    onundo={() => selected && undoOne(selected.id, false)}
    onrestoreanyway={() => selected && undoOne(selected.id, true)}
    disabled={busy || !canAct}
  />
</div>

<p class="sr-only" aria-live="polite">{selected ? `${selected.title}: details shown` : ""}</p>

<ApplyDialogs bind:this={dialogs} tweaks={app.tweaks} returnFocus={applyButton} />
<UndoSummary bind:this={summary} returnFocus={applyButton} onforced={noteChangedSince} />

<style>
  .section {
    flex: 1;
    min-height: 360px;
    display: grid;
    grid-template-columns: minmax(0, 1.25fr) minmax(0, 1fr);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .list-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .list-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 40px;
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }

  .list-head .muted {
    flex: 1;
  }

  .legend {
    display: flex;
    gap: var(--space-2);
  }

  ul {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-y: auto;
  }

  .empty {
    flex: 1;
    padding: var(--space-5) var(--space-4);
    font-size: var(--text-sm);
  }

  .list-foot {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4) var(--space-4);
    border-top: 1px solid var(--border);
  }

  .note {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 20px;
    font-size: var(--text-xs);
    color: var(--muted);
  }

  .note :global(svg) {
    flex: none;
  }

  .progress {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--text);
  }

  .bar {
    height: 2px;
    background: var(--surface-3);
    border-radius: var(--radius-pill);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    transition: width var(--duration) var(--ease);
  }

  .error {
    color: var(--adv);
  }

  .btn-primary[aria-disabled="true"] {
    cursor: progress;
    opacity: 0.7;
  }

  .section > :global(.explainer) {
    border-left: 1px solid var(--border);
    background: var(--surface);
  }
</style>
