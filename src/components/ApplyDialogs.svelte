<script lang="ts">
  // The two questions runApply can ask: what to do about a restore point
  // problem, and whether to go ahead with changes that can't be undone.
  // Pages call `askRestore` / `askIrreversible` through bind:this and pass
  // them to runApply.
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { appName, type RestoreChoice } from "../lib/applyFlow";
  import { formatBytes } from "../lib/format";
  import type { CleanupTarget, RestorePointOutcome, TweakView } from "../lib/types";

  interface Props {
    tweaks: TweakView[];
    returnFocus?: HTMLElement | null;
  }

  let { tweaks, returnFocus = null }: Props = $props();

  type Question =
    | { kind: "restore"; outcome: RestorePointOutcome; canEnable: boolean; resolve: (c: RestoreChoice) => void }
    | { kind: "irreversible"; ids: string[]; cleanup: CleanupTarget[]; resolve: (go: boolean) => void };
  let question = $state<Question | null>(null);

  export function askRestore(outcome: RestorePointOutcome, canEnable: boolean): Promise<RestoreChoice> {
    return new Promise((resolve) => (question = { kind: "restore", outcome, canEnable, resolve }));
  }

  /** `cleanup`: a preset's cleanup targets, confirmed (or skipped) together with the ids. */
  export function askIrreversible(ids: string[], cleanup: CleanupTarget[] = []): Promise<boolean> {
    return new Promise((resolve) => (question = { kind: "irreversible", ids, cleanup, resolve }));
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
      const t = tweaks.find((x) => x.id === id);
      const apps = (t?.changes ?? []).map(appName).filter((n): n is string => n !== null);
      return { id, title: t?.title ?? id, apps };
    });
  }

  const restoreQ = $derived(
    question?.kind === "restore" ? restoreCopy(question.outcome, question.canEnable) : null,
  );
  const irreversibleQ = $derived(question?.kind === "irreversible" ? irreversibleDetail(question.ids) : []);
  const cleanupQ = $derived(question?.kind === "irreversible" ? question.cleanup : []);
  const cleanupTotal = $derived(cleanupQ.reduce((n, t) => n + t.bytes, 0));

  /** Message and button for what the question covers: app removals, files, or both. */
  const irreversibleCopy = $derived.by(() => {
    const apps = irreversibleQ.length > 0;
    const files = cleanupQ.length > 0;
    if (apps && files)
      return {
        message: "Undo can't put these back. You can reinstall most apps from the Microsoft Store; deleted files are gone.",
        confirm: "Remove and clean",
      };
    if (files) return { message: "Undo can't bring deleted files back.", confirm: "Clean" };
    return { message: "Undo can't put these back. You can reinstall most apps from the Microsoft Store.", confirm: "Remove" };
  });
</script>

<ConfirmDialog
  open={question?.kind === "restore"}
  title={restoreQ?.title ?? ""}
  message={restoreQ?.message}
  confirmLabel={restoreQ?.confirm ?? "Continue"}
  secondaryLabel={restoreQ?.secondary}
  onconfirm={() => answer(question?.kind === "restore" && question.canEnable ? "enable" : true)}
  onsecondary={() => answer(true)}
  oncancel={() => answer(false)}
  {returnFocus}
/>

<ConfirmDialog
  open={question?.kind === "irreversible"}
  title="These can't be undone with one click"
  message={irreversibleCopy.message}
  confirmLabel={irreversibleCopy.confirm}
  cancelLabel="Skip these"
  cautious
  onconfirm={() => answer(true)}
  oncancel={() => answer(false)}
  {returnFocus}
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
  {#if cleanupQ.length > 0}
    <div class="irr">
      <h3>Junk files <span class="total num">{formatBytes(cleanupTotal)}</span></h3>
      <ul class="files">
        {#each cleanupQ as t (t.id)}
          <li><span>{t.label}</span><span class="num">{formatBytes(t.bytes)}</span></li>
        {/each}
      </ul>
    </div>
  {/if}
</ConfirmDialog>

<style>
  .irr h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }

  .total {
    margin-left: var(--space-1);
    color: var(--muted);
    font-weight: var(--weight-regular);
  }

  .files {
    margin: 0;
    padding: var(--space-3) var(--space-4);
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    background: var(--bg);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
  }

  .files li {
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .files .num {
    color: var(--muted);
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
