<script lang="ts">
  // One section list row: the switch, and a button that selects the row so
  // the explainer shows it. Up/Down/Home/End on the row buttons are handled
  // by the parent list.
  import LevelPill from "./LevelPill.svelte";
  import type { TweakView } from "../lib/types";

  interface Props {
    tweak: TweakView;
    /** Wanted end state shown by the switch. */
    on: boolean;
    selected: boolean;
    disabled?: boolean;
    /** Id of the explainer panel, for aria-controls. */
    controls: string;
    ontoggle: (on: boolean) => void;
    onselect: () => void;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let { tweak, on, selected, disabled = false, controls, ontoggle, onselect, onkeydown }: Props = $props();

  /** The switch differs from what the PC has now. */
  const changed = $derived(
    on !== (tweak.status.kind === "applied" || tweak.status.kind === "partial") ||
      (on && tweak.status.kind === "partial"),
  );
</script>

<li class:selected>
  <button
    type="button"
    role="switch"
    aria-checked={on}
    aria-label={tweak.title}
    class="switch"
    aria-disabled={disabled || undefined}
    onclick={() => {
      if (!disabled) ontoggle(!on);
      onselect();
    }}
  ><span class="knob"></span></button>
  <button
    type="button"
    class="pick"
    data-row={tweak.id}
    aria-current={selected ? "true" : undefined}
    aria-controls={controls}
    onclick={onselect}
    {onkeydown}
  >
    <span class="title">{tweak.title}</span>
    {#if changed}
      <span class="dot" title="Changes when you apply" aria-hidden="true"></span>
      <span class="sr-only">Changes when you apply.</span>
    {/if}
    <LevelPill level={tweak.level} />
  </button>
</li>

<style>
  li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 44px;
    padding-left: var(--space-4);
    border-bottom: 1px solid var(--border);
    transition: background-color var(--duration) var(--ease);
  }

  li:hover {
    background: var(--surface-2);
  }

  li.selected {
    background: var(--surface-3);
  }

  .pick {
    flex: 1;
    min-width: 0;
    align-self: stretch;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4) var(--space-2) 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font-size: var(--text-md);
    text-align: left;
    cursor: pointer;
  }

  /* Keyboard focus outlines the whole row, not just the text button. */
  .pick:focus-visible {
    outline: none;
  }

  li:has(.pick:focus-visible) {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .title {
    flex: 1;
    min-width: 0;
  }

  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text);
  }
</style>
