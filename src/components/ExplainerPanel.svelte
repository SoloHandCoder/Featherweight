<script lang="ts">
  // What the selected tweak does, exactly: its description, one line per
  // changed value, its status, and the per-tweak controls.
  import { CircleAlert, RotateCcw, Undo2 } from "lucide-svelte";
  import type { TweakView } from "../lib/types";

  interface Props {
    id: string;
    tweak: TweakView | null;
    allUsers: boolean;
    onallusers: (on: boolean) => void;
    onundo: () => void;
    /** Set when the last undo of this tweak found a value changed by something else. */
    changedSince?: string | null;
    onrestoreanyway: () => void;
    disabled?: boolean;
  }

  let {
    id,
    tweak,
    allUsers,
    onallusers,
    onundo,
    changedSince = null,
    onrestoreanyway,
    disabled = false,
  }: Props = $props();

  const uid = $props.id();
  const on = $derived(tweak?.status.kind === "applied" || tweak?.status.kind === "partial");
</script>

<aside {id} class="explainer" aria-labelledby={tweak ? `${uid}-title` : undefined} aria-label={tweak ? undefined : "Details"}>
  {#if !tweak}
    <p class="empty muted">Select a change to see exactly what it does.</p>
  {:else}
    <h2 id="{uid}-title">{tweak.title}</h2>
    {#if tweak.description}
      <p class="desc">{tweak.description}</p>
    {/if}

    <section aria-labelledby="{uid}-changes">
      <h3 id="{uid}-changes">What it changes</h3>
      <ul class="changes">
        {#each tweak.changes as line, i (i)}<li>{line}</li>{/each}
      </ul>
    </section>

    <dl class="facts">
      <div>
        <dt>Status</dt>
        <dd>
          {#if tweak.status.kind === "applied"}
            <span class="ok">Applied</span>
          {:else if tweak.status.kind === "partial"}
            <span class="ok">Applied</span>
            <span class="note">Partly applied: some of its values are not set yet.</span>
          {:else}
            Not applied
          {/if}
        </dd>
      </div>
      {#if tweak.perUser}
        <div>
          <dt>Applies to</dt>
          <dd>Your account only</dd>
        </div>
      {/if}
      {#if tweak.irreversible}
        <div>
          <dt>Undo</dt>
          <dd class="warn">Can't be undone with one click</dd>
        </div>
      {/if}
    </dl>

    {#if tweak.allUsersOptional}
      <label class="check">
        <input
          type="checkbox"
          checked={allUsers}
          {disabled}
          onchange={(e) => onallusers(e.currentTarget.checked)}
        />
        <span>Also remove for all users</span>
      </label>
    {/if}

    {#if changedSince}
      <div class="changed" role="status">
        <CircleAlert size={16} strokeWidth={1.75} aria-hidden="true" />
        <p>Something else changed {changedSince} since Featherweight set it, so it was left alone.</p>
      </div>
    {/if}

    {#if on || changedSince}
      <div class="actions">
        {#if changedSince}
          <button type="button" class="btn btn-ghost btn-sm" {disabled} onclick={onrestoreanyway}>
            <RotateCcw size={14} strokeWidth={1.75} aria-hidden="true" />
            Restore anyway
          </button>
        {:else}
          <button type="button" class="btn btn-ghost btn-sm" {disabled} onclick={onundo}>
            <Undo2 size={14} strokeWidth={1.75} aria-hidden="true" />
            Undo this one
          </button>
        {/if}
      </div>
    {/if}
  {/if}
</aside>

<style>
  .explainer {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-5);
    overflow-y: auto;
    min-height: 0;
  }

  .empty {
    margin: auto 0;
    text-align: center;
    font-size: var(--text-sm);
  }

  h2 {
    font-size: var(--text-lg);
    line-height: var(--leading-tight);
  }

  .desc {
    margin-top: calc(-1 * var(--space-2));
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: var(--leading);
  }

  h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--muted);
  }

  .changes {
    margin: 0;
    padding: var(--space-3);
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: var(--leading);
    color: color-mix(in srgb, var(--text) 82%, var(--muted));
    overflow-wrap: anywhere;
    max-height: 180px;
    overflow-y: auto;
  }

  .facts {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }

  .facts div {
    display: flex;
    gap: var(--space-3);
  }

  dt {
    flex: none;
    width: 72px;
    color: var(--muted);
  }

  dd {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .ok {
    color: var(--safe);
  }

  .warn {
    color: var(--adv);
  }

  .note {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .check input {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--accent);
  }

  .changed {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-3);
    background: var(--adv-bg);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
  }

  .changed :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--adv);
  }

  .actions {
    display: flex;
    gap: var(--space-2);
  }
</style>
