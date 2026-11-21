<script lang="ts">
  import { CircleCheck, TriangleAlert } from "lucide-svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import { backend } from "../lib/backend";
  import { app, message, startBusy, stopBusy, BACKEND_UNAVAILABLE } from "../lib/state.svelte";
  import { formatBytes } from "../lib/format";
  import { nextSelection, selectedBytes, selectedTargets } from "../lib/cleanupSelection";
  import type { CleanupReport, CleanupTarget } from "../lib/types";

  const HINTS: Record<string, string> = {
    "wu-cache": "Windows Update is paused for a moment while this is cleaned.",
  };

  let targets = $state<CleanupTarget[]>([]);
  let selected = $state<Set<string>>(new Set());
  let scanning = $state(true);
  let running = $state(false);
  let error = $state<string | null>(null);
  let report = $state<CleanupReport | null>(null);
  let confirmOpen = $state(false);
  let cleanButton = $state<HTMLButtonElement | null>(null);

  const chosen = $derived(selectedTargets(targets, selected).filter((t) => t.bytes > 0));
  const total = $derived(selectedBytes(targets, selected));
  const busy = $derived(scanning || running);
  const canClean = $derived(app.backendAvailable && !busy && chosen.length > 0 && total > 0);

  /** `keepError`: the post-clean rescan must not erase a failed run's message. */
  async function scan(keepError = false) {
    scanning = true;
    if (!keepError) error = null;
    try {
      const found = await backend.api.scanCleanup();
      selected = nextSelection(found, new Set(targets.map((t) => t.id)), selected);
      targets = found;
    } catch (e) {
      error = message(e);
    } finally {
      scanning = false;
    }
  }

  // Wait for the first refresh to settle: in the dev mock it swaps the backend in.
  let started = false;
  $effect(() => {
    if (started) return;
    if (!app.backendAvailable) {
      scanning = false;
      return;
    }
    if (app.overview !== null || app.error !== null) {
      started = true;
      void scan();
    }
    // Unavailable-state text comes from BACKEND_UNAVAILABLE.
  });

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }

  async function run() {
    if (running || scanning) return;
    confirmOpen = false;
    if (!startBusy({ kind: "cleanup", label: "Cleaning" })) return;
    running = true;
    error = null;
    report = null;
    try {
      report = await backend.api.runCleanup(chosen.map((t) => t.id));
    } catch (e) {
      error = message(e);
    } finally {
      running = false;
      stopBusy();
    }
    await scan(true);
  }

  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
</script>

<section class="cleanup" aria-busy={busy}>
  <p class="intro muted">Remove files Windows and apps leave behind. Your documents are never touched.</p>

  {#if error}
    <div class="notice" role="alert">
      <TriangleAlert size={16} strokeWidth={1.75} aria-hidden="true" />
      <p>{error}</p>
      {#if app.backendAvailable}
        <button type="button" class="btn btn-ghost btn-sm" onclick={() => scan()} disabled={busy}>Retry</button>
      {/if}
    </div>
  {/if}

  {#if report}
    <div class="result" role="status">
      <CircleCheck size={18} strokeWidth={1.75} aria-hidden="true" />
      <div>
        <p class="result-title">Freed {formatBytes(report.freedBytes)}.</p>
        {#if report.skippedFiles > 0}
          <p class="muted">{plural(report.skippedFiles, "file was", "files were")} in use and {report.skippedFiles === 1 ? "was" : "were"} skipped.</p>
        {/if}
      </div>
    </div>
  {/if}

  <div class="list" aria-label="Cleanup targets">
    {#if scanning && targets.length === 0}
      {#each [0, 1, 2, 3] as i (i)}
        <div class="row skeleton" aria-hidden="true">
          <span class="ph box"></span>
          <span class="ph label" style:width="{[34, 44, 28, 40][i]}%"></span>
          <span class="ph size"></span>
        </div>
      {/each}
      <span class="sr-only">Scanning for files to clean</span>
    {:else if !app.backendAvailable}
      <p class="empty muted">{BACKEND_UNAVAILABLE}</p>
    {:else if targets.length === 0 && !error}
      <p class="empty muted">Nothing to clean up right now.</p>
    {:else}
      {#each targets as t (t.id)}
        <label class="row" class:dim={running}>
          <input
            type="checkbox"
            checked={selected.has(t.id)}
            disabled={busy || t.bytes === 0}
            onchange={() => toggle(t.id)}
          />
          <span class="text">
            <span class="label">{t.label}</span>
            {#if HINTS[t.id]}
              <span class="hint muted">{HINTS[t.id]}</span>
            {/if}
          </span>
          <span class="size num" class:muted={t.bytes === 0}>{formatBytes(t.bytes)}</span>
        </label>
      {/each}
    {/if}
  </div>

  <footer class="bar">
    <p class="total">
      {#if scanning && targets.length === 0}
        <span class="muted">Scanning</span>
      {:else}
        <span class="muted">Selected</span> <span class="num">{formatBytes(total)}</span>
      {/if}
    </p>
    <button
      type="button"
      class="btn btn-primary"
      bind:this={cleanButton}
      disabled={!canClean}
      onclick={() => (confirmOpen = true)}
    >
      {running ? "Cleaning" : `Clean ${formatBytes(total)}`}
    </button>
  </footer>
</section>

<ConfirmDialog
  open={confirmOpen}
  title="This can't be undone"
  message="These files will be deleted:"
  items={chosen.map((t) => `${t.label}: ${formatBytes(t.bytes)}`)}
  confirmLabel="Clean"
  cautious
  returnFocus={cleanButton}
  onconfirm={run}
  oncancel={() => (confirmOpen = false)}
/>

<style>
  .cleanup {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    max-width: 720px;
  }

  .intro {
    font-size: var(--text-md);
  }

  .notice {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    min-height: 40px;
    background: var(--adv-bg);
    border-radius: var(--radius);
    font-size: var(--text-sm);
  }

  .notice p {
    flex: 1;
  }

  .notice :global(svg) {
    flex: none;
    color: var(--adv);
  }

  .result {
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
    padding: var(--space-3) var(--space-4);
    background: var(--safe-bg);
    border-radius: var(--radius);
    font-size: var(--text-sm);
  }

  .result :global(svg) {
    flex: none;
    margin-top: 1px;
    color: var(--safe);
  }

  .result-title {
    font-weight: var(--weight-semibold);
    font-size: var(--text-md);
  }

  .result .muted {
    color: var(--text);
    opacity: 0.8;
  }

  .list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    overflow: hidden;
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 52px;
    padding: var(--space-2) var(--space-4);
    cursor: pointer;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .row:not(.skeleton):hover {
    background: var(--surface-2);
  }

  .row.dim {
    opacity: 0.6;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .label {
    font-size: var(--text-md);
  }

  .hint {
    font-size: var(--text-xs);
  }

  .size {
    font-size: var(--text-md);
    min-width: 72px;
    text-align: right;
  }

  .empty {
    padding: var(--space-5) var(--space-4);
  }

  /* Real checkbox, restyled. */
  input[type="checkbox"] {
    appearance: none;
    flex: none;
    width: 18px;
    height: 18px;
    margin: 0;
    border: 1.5px solid var(--border-strong);
    border-radius: 5px;
    background: var(--bg);
    display: grid;
    place-content: center;
    cursor: pointer;
    transition:
      background-color var(--duration) var(--ease),
      border-color var(--duration) var(--ease);
  }

  input[type="checkbox"]::after {
    content: "";
    width: 9px;
    height: 5px;
    border-left: 2px solid var(--on-accent);
    border-bottom: 2px solid var(--on-accent);
    transform: translateY(-1px) rotate(-45deg);
    opacity: 0;
  }

  input[type="checkbox"]:checked {
    background: var(--accent);
    border-color: var(--accent);
  }

  input[type="checkbox"]:checked::after {
    opacity: 1;
  }

  input[type="checkbox"]:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  input[type="checkbox"]:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  /* Skeleton placeholders: static, muted. */
  .ph {
    display: block;
    height: 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-3);
  }

  .ph.box {
    width: 18px;
    height: 18px;
    flex: none;
  }

  .ph.label {
    height: 14px;
    flex: none;
  }

  .skeleton .ph.size {
    width: 56px;
    margin-left: auto;
  }

  .skeleton .ph.label {
    margin-right: auto;
  }

  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .total {
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }

  .total .muted {
    font-weight: var(--weight-regular);
    font-size: var(--text-md);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
