<script lang="ts">
  import { Gamepad2, LifeBuoy, ListChecks, Zap } from "@lucide/svelte";
  import StatTile from "../components/StatTile.svelte";
  import PresetCard from "../components/PresetCard.svelte";
  import LevelPill from "../components/LevelPill.svelte";
  import ApplyDialogs from "../components/ApplyDialogs.svelte";
  import ResultSummary from "../components/ResultSummary.svelte";
  import { backend } from "../lib/backend";
  import { app, changed, message, startBusy, stopBusy } from "../lib/state.svelte";
  import { groupApplyReport, runPreset, type PresetResult, type ResultGroup } from "../lib/applyFlow";
  import { formatBytes } from "../lib/format";
  import type { Preset, Progress, TweakView } from "../lib/types";

  const PRESETS: { id: Preset; name: string; description: string; icon: typeof Zap }[] = [
    { id: "everyday", name: "Everyday", description: "Faster, quieter PC. Only safe changes.", icon: Zap },
    { id: "gaming", name: "Gaming", description: "Everyday plus FPS and input-lag tweaks.", icon: Gamepad2 },
  ];

  let preset = $state<Preset>("everyday");
  /** Ids the user switched off in the review list, per preset. */
  let excluded = $state<Record<Preset, string[]>>({ everyday: [], gaming: [] });
  let reviewOpen = $state(false);

  const busy = $derived(app.busy !== null);
  let nothingChanged = $state(false);
  let dialogs: ApplyDialogs | undefined = $state();
  let phase = $state<"restore" | "scan" | "apply" | "clean" | null>(null);
  let progress = $state<Progress | null>(null);
  let applyError = $state<string | null>(null);
  let applyButton = $state<HTMLButtonElement | null>(null);

  let result = $state<{ title: string; summary: string; groups: ResultGroup[] } | null>(null);

  const byId = $derived(new Map(app.tweaks.map((t) => [t.id, t])));
  const presetName = $derived(PRESETS.find((p) => p.id === preset)!.name);

  /** The preset's tweaks that still need doing on this PC. */
  const pending = $derived(
    app.presets[preset]
      .map((id) => byId.get(id))
      .filter((t): t is TweakView => !!t && t.status.kind !== "applied" && t.status.kind !== "notApplicable"),
  );
  const alreadyDone = $derived(app.presets[preset].length - pending.length);
  const selected = $derived(pending.filter((t) => !excluded[preset].includes(t.id)).map((t) => t.id));
  const loaded = $derived(app.overview !== null);
  /** Presets also clean up junk files; the scan at apply time decides exactly what. */
  const junkBytes = $derived(app.overview?.stats.junkBytes ?? 0);
  const canApply = $derived(
    app.backendAvailable && loaded && !app.overview?.journalError && (selected.length > 0 || junkBytes > 0),
  );

  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const applyLabel = $derived(
    !loaded
      ? `Apply ${presetName}`
      : selected.length === 0 && junkBytes > 0
        ? `Clean up with ${presetName}`
        : `Apply ${presetName} (${plural(selected.length, "change", "changes")})`,
  );

  function presetMeta(p: Preset): string | undefined {
    if (!loaded) return undefined;
    const ids = app.presets[p];
    const todo = ids.filter((id) => {
      const s = byId.get(id)?.status.kind;
      return s !== undefined && s !== "applied" && s !== "notApplicable";
    }).length;
    if (ids.length === 0) return undefined;
    if (todo === 0) return "All applied";
    return `${plural(todo, "change", "changes")} to make`;
  }

  function onPresetKey(e: KeyboardEvent) {
    if (busy) return;
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) return;
    e.preventDefault();
    const i = PRESETS.findIndex((p) => p.id === preset);
    const step = e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 1;
    preset = PRESETS[(i + step + PRESETS.length) % PRESETS.length].id;
    const group = (e.currentTarget as HTMLElement).parentElement;
    queueMicrotask(() => group?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
  }

  function toggle(id: string) {
    if (busy) return;
    const list = excluded[preset];
    excluded[preset] = list.includes(id) ? list.filter((x) => x !== id) : [...list, id];
  }

  function statusText(t: TweakView): string {
    return t.status.kind === "partial" ? "Partly applied" : "Not applied";
  }

  /** Result groups for a preset run: the tweak outcomes, then the cleanup. */
  function presetGroups(r: PresetResult): ResultGroup[] {
    const groups = r.apply ? groupApplyReport(r.apply, app.tweaks) : [];
    // Cleanup is one line, so it goes first, above a possibly long list.
    if (r.cleanup) {
      const skipped = r.cleanup.skippedFiles;
      groups.unshift({
        label: "Cleaned up",
        tone: "ok",
        hideCount: true,
        items: [
          {
            title: `Freed ${formatBytes(r.cleanup.freedBytes)}.`,
            detail:
              skipped > 0
                ? `${plural(skipped, "file was", "files were")} in use and ${skipped === 1 ? "was" : "were"} skipped.`
                : undefined,
          },
        ],
      });
    }
    if (r.cleanupError)
      groups.push({ label: "Cleanup failed", tone: "bad", hideCount: true, items: [{ title: r.cleanupError }] });
    return groups;
  }

  function summarize(groups: ResultGroup[], total: number, name: string) {
    const count = (label: string) => groups.find((g) => g.label === label)?.items.length ?? 0;
    const applied = count("Applied");
    const failed = count("Failed") + count("Cleanup failed");
    const cleaned = count("Cleaned up") > 0;
    if (failed > 0) {
      return {
        title: "Some changes didn't go through",
        summary:
          total > 0 ? `${applied} of ${total} applied. Each one that didn't says why.` : "The details say why.",
      };
    }
    if (applied === 0 && !cleaned)
      return { title: "Nothing needed changing", summary: "This PC already had these settings." };
    if (applied === 0) return { title: "Junk files cleaned up", summary: "Every setting was already in place." };
    return { title: `${plural(applied, "change", "changes")} applied`, summary: `${name} is set up on this PC.` };
  }

  async function apply() {
    if (busy || !canApply) return;
    // Snapshot so switching cards mid-apply can't change what's shown.
    const name = presetName;
    const ids = selected;
    if (!startBusy({ kind: "apply", label: `Applying ${name}` })) return;
    applyError = null;
    nothingChanged = false;
    let unlisten: (() => void) | undefined;
    const api = backend.api;
    try {
      const run = await runPreset(ids, {
        tweaks: app.tweaks,
        ensureRestorePoint: () => {
          phase = "restore";
          return api.ensureRestorePoint();
        },
        enableSystemRestore: () => api.enableSystemRestore(),
        confirmRestoreIssue: (outcome, { canEnable }) => {
          phase = null;
          return dialogs!.askRestore(outcome, canEnable);
        },
        scanCleanup: () => {
          phase = "scan";
          return api.scanCleanup();
        },
        confirmIrreversible: (irr, cleanup) => {
          phase = null;
          return dialogs!.askIrreversible(irr, cleanup);
        },
        applyTweaks: async (chosen, allUsers) => {
          phase = "apply";
          progress = { done: 0, total: chosen.length, current: "" };
          unlisten = await api.onApplyProgress((p) => (progress = p));
          return api.applyTweaks(chosen, allUsers);
        },
        runCleanup: (targets) => {
          phase = "clean";
          return api.runCleanup(targets);
        },
      });
      const groups = run ? presetGroups(run) : [];
      if (run && groups.length > 0) {
        result = { ...summarize(groups, run.apply?.results.length ?? 0, name), groups };
      } else {
        nothingChanged = true;
      }
    } catch (e) {
      applyError = message(e);
    } finally {
      unlisten?.();
      stopBusy();
      phase = null;
      progress = null;
    }
    await changed();
  }
</script>

<div class="home">
  <section class="stats" aria-label="This PC">
    <StatTile
      label="Startup apps"
      value={app.overview ? String(app.overview.stats.startupEnabled) : null}
      hint="Start with Windows"
    />
    <StatTile
      label="Background services"
      value={app.overview ? String(app.overview.stats.servicesRunning) : null}
      hint="Running now"
    />
    <StatTile
      label="Junk files"
      value={app.overview ? formatBytes(app.overview.stats.junkBytes) : null}
      hint="Safe to clean"
    />
  </section>

  <section class="presets">
    <h2 id="preset-heading">Pick a preset</h2>
    <div class="cards" role="radiogroup" aria-labelledby="preset-heading">
      {#each PRESETS as p (p.id)}
        <PresetCard
          name={p.name}
          description={p.description}
          icon={p.icon}
          selected={preset === p.id}
          meta={presetMeta(p.id)}
          disabled={busy}
          onselect={() => !busy && (preset = p.id)}
          onkeydown={onPresetKey}
        />
      {/each}
    </div>
    {#if loaded && junkBytes > 0}
      <p class="also">Both presets also clean up <span class="num">{formatBytes(junkBytes)}</span> of junk files.</p>
    {/if}
  </section>

  <div class="go">
    <div class="actions">
      <button
        bind:this={applyButton}
        type="button"
        class="btn btn-primary apply num"
        aria-disabled={busy || !canApply || undefined}
        data-busy={busy || undefined}
        onclick={apply}
      >
        {busy ? (app.busy?.label ?? "Working") : applyLabel}
      </button>
      <button
        type="button"
        class="btn btn-ghost review-toggle"
        aria-expanded={reviewOpen}
        aria-controls={reviewOpen ? "review-list" : undefined}
        disabled={!loaded}
        onclick={() => (reviewOpen = !reviewOpen)}
      >
        <ListChecks size={16} strokeWidth={1.75} aria-hidden="true" />
        {reviewOpen ? "Hide list" : "Review list"}
      </button>
    </div>

    <div class="note" aria-live="polite">
      {#if phase === "restore"}
        <p>Making a restore point</p>
      {:else if phase === "scan"}
        <p>Looking for junk files</p>
      {:else if phase === "clean"}
        <p>Cleaning up junk files</p>
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
      {:else if applyError}
        <p class="error" role="alert">Couldn't apply: {applyError}</p>
      {:else if nothingChanged}
        <p role="status">Nothing was changed.</p>
      {:else}
        <LifeBuoy size={14} strokeWidth={1.75} aria-hidden="true" />
        <p>A restore point is made before the first change.</p>
      {/if}
    </div>
  </div>
  {#if reviewOpen}
    <section id="review-list" class="review" aria-labelledby="review-heading">
      <header>
        <h2 id="review-heading">{presetName}</h2>
        <span class="muted num">
          {selected.length} of {pending.length} selected{alreadyDone > 0 ? ` · ${alreadyDone} already applied` : ""}
        </span>
      </header>
      {#if pending.length === 0}
        <p class="muted empty">Everything in {presetName} is already applied on this PC.</p>
      {:else}
        <ul>
          {#each pending as t (t.id)}
            {@const on = !excluded[preset].includes(t.id)}
            <li>
              <button
                type="button"
                role="switch"
                aria-checked={on}
                aria-label={t.title}
                class="switch"
                aria-disabled={busy || undefined}
                onclick={() => toggle(t.id)}
              ><span class="knob"></span></button>
              <span class="title">{t.title}</span>
              {#if t.irreversible}<span class="tag">Can't be undone</span>{/if}
              <span class="status">{statusText(t)}</span>
              <LevelPill level={t.level} />
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}

</div>

<ApplyDialogs bind:this={dialogs} tweaks={app.tweaks} returnFocus={applyButton} />

<ResultSummary
  open={result !== null}
  title={result?.title ?? ""}
  summary={result?.summary}
  groups={result?.groups ?? []}
  onclose={() => (result = null)}
  returnFocus={applyButton}
/>

<style>
  .home {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    min-height: 0;
  }

  /* One strip split by rules, not three cards. */
  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .stats > :global(* + *) {
    border-left: 1px solid var(--border);
  }

  .presets {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .presets h2 {
    font-size: var(--text-md);
    color: var(--muted);
  }

  .cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }

  .also {
    font-size: var(--text-sm);
    color: var(--muted);
  }

  .go {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
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

  .review {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }

  .review header {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
  }

  .review h2 {
    font-size: var(--text-md);
  }

  .review header span {
    font-size: var(--text-sm);
  }

  .review ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .review li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 40px;
    padding: var(--space-1) var(--space-4);
    font-size: var(--text-sm);
  }

  .review li + li {
    border-top: 1px solid var(--border);
  }

  .empty {
    padding: var(--space-4);
    font-size: var(--text-sm);
  }

  .title {
    flex: 1;
    min-width: 0;
  }

  .tag,
  .status {
    font-size: var(--text-xs);
    color: var(--muted);
    white-space: nowrap;
  }

  .tag {
    color: var(--adv);
  }

  .actions {
    display: flex;
    gap: var(--space-3);
  }

  .apply {
    flex: 1;
  }

  /* Same width for "Review list" and "Hide list", so Apply doesn't jump. */
  .review-toggle {
    min-width: 128px;
  }

</style>
