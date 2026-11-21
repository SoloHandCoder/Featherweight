<script lang="ts">
  // One option of the "Pick a preset" radio group. Only the selected card is
  // in the tab order; the parent moves the selection with arrow keys.
  import type { Zap } from "lucide-svelte";

  interface Props {
    name: string;
    description: string;
    icon: typeof Zap;
    selected: boolean;
    /** e.g. "12 changes to make"; omitted while loading. */
    meta?: string;
    onselect: () => void;
    onkeydown?: (e: KeyboardEvent) => void;
    /** Set while a change is running. */
    disabled?: boolean;
  }

  let { name, description, icon: Icon, selected, meta, onselect, onkeydown, disabled = false }: Props = $props();
</script>

<button
  type="button"
  role="radio"
  aria-checked={selected}
  tabindex={selected ? 0 : -1}
  class="card"
  class:selected
  {disabled}
  onclick={onselect}
  {onkeydown}
>
  <span class="head">
    <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
    <span class="name">{name}</span>
    <span class="dot" aria-hidden="true"></span>
  </span>
  <span class="desc">{description}</span>
  {#if meta}<span class="meta num">{meta}</span>{/if}
</button>

<style>
  .card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-4);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    text-align: left;
    cursor: pointer;
    transition:
      border-color var(--duration) var(--ease),
      background-color var(--duration) var(--ease);
  }

  .card:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .card:not(:disabled):hover {
    border-color: var(--border-strong);
    background: var(--surface-3);
  }

  .card.selected {
    border-color: var(--accent);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
  }

  .head :global(svg) {
    flex: none;
    color: var(--muted);
  }

  .selected .head :global(svg) {
    color: var(--accent);
  }

  .name {
    flex: 1;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }

  .dot {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1.5px solid var(--border-strong);
    flex: none;
  }

  .selected .dot {
    border: 4px solid var(--accent);
  }

  .desc {
    font-size: var(--text-sm);
    color: var(--muted);
  }

  .meta {
    margin-top: var(--space-1);
    font-size: var(--text-xs);
    color: var(--muted);
  }
</style>
