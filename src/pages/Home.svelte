<script lang="ts">
  import { Gamepad2, LifeBuoy, ListChecks, Zap } from "lucide-svelte";
  import StatTile from "../components/StatTile.svelte";
  import PresetCard from "../components/PresetCard.svelte";
  import LevelPill from "../components/LevelPill.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import ResultSummary from "../components/ResultSummary.svelte";
  import { backend } from "../lib/backend";
  import { app, message, refresh } from "../lib/state.svelte";
  import { appName, groupApplyReport, runApply, type ResultGroup, type RestoreChoice } from "../lib/applyFlow";
  import { formatBytes } from "../lib/format";
  import type { Preset, Progress, RestorePointOutcome, TweakView } from "../lib/types";

  const PRESETS: { id: Preset; name: string; description: string; icon: typeof Zap }[] = [
    { id: "everyday", name: "Everyday", description: "Faster, quieter PC. Only safe changes.", icon: Zap },
    { id: "gaming", name: "Gaming", description: "Everyday plus FPS and input-lag tweaks.", icon: Gamepad2 },
  ];

  let preset = $state<Preset>("everyday");
  /** Ids the user switched off in the review list, per preset. */
  let excluded = $state<Record<Preset, string[]>>({ everyday: [], gaming: [] });
  let reviewOpen = $state(false);

  let busy = $state(false);
  let phase = $state<"restore" | "apply" | null>(null);
  let progress = $state<Progress | null>(null);
  let applyError = $state<string | null>(null);
  let applyButton = $state<HTMLButtonElement | null>(null);

  type Question =
    | { kind: "restore"; outcome: RestorePointOutcome; canEnable: boolean; resolve: (c: RestoreChoice) => void }
    | { kind: "irreversible"; ids: string[]; resolve: (go: boolean) => void };
  let question = $state<Question | null>(null);
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
  const canApply = $derived(
    app.backendAvailable && loaded && !app.overview?.journalError && selected.length > 0,
  );

  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const applyLabel = $derived(`Apply ${presetName} (${plural(selected.length, "change", "changes")})`);

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
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) return;
    e.preventDefault();
    const i = PRESETS.findIndex((p) => p.id === preset);
    const step = e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 1;
    preset = PRESETS[(i + step + PRESETS.length) % PRESETS.length].id;
    const group = (e.currentTarget as HTMLElement).parentElement;
    queueMicrotask(() => group?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
  }

  function toggle(id: string) {
    const list = excluded[preset];
    excluded[preset] = list.includes(id) ? list.filter((x) => x !== id) : [...list, id];
  }

  function statusText(t: TweakView): string {
    return t.status.kind === "partial" ? "Partly applied" : "Not applied";
  }

  function askRestore(outcome: RestorePointOutcome, canEnable: boolean): Promise<RestoreChoice> {
    return new Promise((resolve) => (question = { kind: "restore", outcome, canEnable, resolve }));
  }

  function askIrreversible(ids: string[]): Promise<boolean> {
    return new Promise((resolve) => (question = { kind: "irreversible", ids, resolve }));
  }

  function answer(value: RestoreChoice) {
    const q = question;
    question = null;
    if (q?.kind === "restore") q.resolve(value);
    else if (q?.kind === "irreversible") q.resolve(value === true);
  }

  function restoreCopy(o: RestorePointOutcome, canEnable: boolean) {
    const tail = "Featherweight keeps its own record, so it can still undo its changes.";
    switch (o.kind) {
      case "throttled":
        return {
          title: "No new restore point",
          message: `Windows makes at most one restore point a day, and this PC made one recently. ${tail}`,
          confirm: "Continue",
        };
      case "disabled":
        // canEnable is false only after a turn-on attempt that didn't stick.
        return canEnable
          ? {
              title: "System Restore is off",
              message: `Windows can't make a restore point because System Restore is turned off on this PC. ${tail}`,
              confirm: "Turn on System Restore",
              secondary: "Continue without one",
            }
          : {
              title: "System Restore is still off",
              message: `Windows didn't make a restore point after turning System Restore on. ${tail}`,
              confirm: "Continue without one",
            };
      default:
        return {
          title: "Couldn't make a restore point",
          message: `${o.kind === "failed" ? o.message.replace(/\.?$/, ".") : ""} ${tail}`.trim(),
          confirm: "Continue without one",
        };
    }
  }

  /** Irreversible tweaks with the apps each one removes. */
  function irreversibleDetail(ids: string[]) {
    return ids.map((id) => {
      const t = byId.get(id);
      const apps = (t?.changes ?? []).map(appName).filter((n): n is string => n !== null);
      return { id, title: t?.title ?? id, apps };
    });
  }

  function summarize(groups: ResultGroup[], total: number) {
    const count = (label: string) => groups.find((g) => g.label === label)?.items.length ?? 0;
    const applied = count("Applied");
    const failed = count("Failed");
    if (failed > 0) {
      return {
        title: "Some changes didn't apply",
        summary: `${applied} of ${total} applied. The ones that failed say why.`,
      };
    }
    if (applied === 0) return { title: "Nothing needed changing", summary: "This PC already had these settings." };
    return { title: `${plural(applied, "change", "changes")} applied`, summary: `${presetName} is set up on this PC.` };
  }

  async function apply() {
    if (busy || !canApply) return;
    busy = true;
    applyError = null;
    let unlisten: (() => void) | undefined;
    const api = backend.api;
    try {
      const report = await runApply(selected, [], {
        tweaks: app.tweaks,
        ensureRestorePoint: () => {
          phase = "restore";
          return api.ensureRestorePoint();
        },
        enableSystemRestore: () => api.enableSystemRestore(),
        confirmRestoreIssue: (outcome, { canEnable }) => {
          phase = null;
          return askRestore(outcome, canEnable);
        },
        confirmIrreversible: (ids) => {
          phase = null;
          return askIrreversible(ids);
        },
        applyTweaks: async (ids, allUsers) => {
          phase = "apply";
          progress = { done: 0, total: ids.length, current: "" };
          unlisten = await api.onApplyProgress((p) => (progress = p));
          return api.applyTweaks(ids, allUsers);
        },
      });
      if (report) {
        const groups = groupApplyReport(report, app.tweaks);
        result = { ...summarize(groups, report.results.length), groups };
      }
    } catch (e) {
      applyError = message(e);
    } finally {
      unlisten?.();
      busy = false;
      phase = null;
      progress = null;
    }
    await refresh();
  }

  const restoreQ = $derived(
    question?.kind === "restore" ? restoreCopy(question.outcome, question.canEnable) : null,
  );
  const irreversibleQ = $derived(question?.kind === "irreversible" ? irreversibleDetail(question.ids) : []);
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
          onselect={() => (preset = p.id)}
          onkeydown={onPresetKey}
        />
      {/each}
    </div>
  </section>

  <div class="note" aria-live="polite">
    {#if phase === "restore"}
      <LifeBuoy size={16} strokeWidth={1.75} aria-hidden="true" />
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
    {:else}
      <LifeBuoy size={16} strokeWidth={1.75} aria-hidden="true" />
      <p>A restore point is made before the first change.</p>
    {/if}
  </div>

  {#if applyError}
    <p class="error" role="alert">Couldn't apply: {applyError}</p>
  {/if}

  <div class="actions">
    <button
      bind:this={applyButton}
      type="button"
      class="btn btn-primary apply num"
      disabled={!canApply && !busy}
      aria-disabled={busy || undefined}
      onclick={apply}
    >
      {busy ? `Applying ${presetName}` : applyLabel}
    </button>
    <button
      type="button"
      class="btn btn-ghost"
      aria-expanded={reviewOpen}
      aria-controls={reviewOpen ? "review-list" : undefined}
      disabled={!loaded}
      onclick={() => (reviewOpen = !reviewOpen)}
    >
      <ListChecks size={16} strokeWidth={1.75} aria-hidden="true" />
      {reviewOpen ? "Hide list" : "Review list"}
    </button>
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
                disabled={busy}
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

<ConfirmDialog
  open={question?.kind === "restore"}
  title={restoreQ?.title ?? ""}
  message={restoreQ?.message}
  confirmLabel={restoreQ?.confirm ?? "Continue"}
  secondaryLabel={restoreQ?.secondary}
  onconfirm={() => answer(question?.kind === "restore" && question.canEnable ? "enable" : true)}
  onsecondary={() => answer(true)}
  oncancel={() => answer(false)}
  returnFocus={applyButton}
/>

<ConfirmDialog
  open={question?.kind === "irreversible"}
  title="These can't be undone with one click"
  message="Undo can't put these back. You can reinstall most apps from the Microsoft Store."
  confirmLabel="Remove and clean"
  cancelLabel="Skip these"
  cautious
  onconfirm={() => answer(true)}
  oncancel={() => answer(false)}
  returnFocus={applyButton}
>
  {#each irreversibleQ as t (t.id)}
    <div class="irr">
      <h3>{t.title}</h3>
      {#if t.apps.length > 0}
        <ul class="apps">
          {#each t.apps as name (name)}<li>{name}</li>{/each}
        </ul>
      {/if}
    </div>
  {/each}
</ConfirmDialog>

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

  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--space-3);
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

  .note {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 44px;
    padding: var(--space-2) var(--space-4);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: var(--text-sm);
    color: var(--muted);
  }

  .note :global(svg) {
    flex: none;
  }

  .progress {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
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
    font-size: var(--text-sm);
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

  .switch {
    position: relative;
    flex: none;
    width: 32px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--border-strong);
    cursor: pointer;
    transition: background-color var(--duration) var(--ease);
  }

  .switch .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--muted);
    transition:
      transform var(--duration) var(--ease),
      background-color var(--duration) var(--ease);
  }

  .switch[aria-checked="true"] {
    background: var(--accent);
  }

  .switch[aria-checked="true"] .knob {
    transform: translateX(14px);
    background: var(--on-accent);
  }

  .switch:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .actions {
    display: flex;
    gap: var(--space-3);
  }

  .apply {
    flex: 1;
  }

  .apply[aria-disabled="true"] {
    cursor: progress;
    opacity: 0.7;
  }

  .irr h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }

  .apps {
    margin: 0;
    padding: var(--space-3) var(--space-4);
    list-style: none;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-1) var(--space-4);
    background: var(--bg);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
  }
</style>
