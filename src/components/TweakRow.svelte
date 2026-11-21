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
    {disabled}
    onclick={() => {
      ontoggle(!on);
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

  .pick:focus-visible {
    outline-offset: -2px;
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
    background: var(--accent);
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

  .knob {
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
</style>
