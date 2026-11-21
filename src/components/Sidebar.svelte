<script lang="ts">
  import { Activity, Gamepad2, House, RotateCcw, Sparkles, Trash2 } from "lucide-svelte";
  import Logo from "./Logo.svelte";
  import type { Page } from "../lib/state.svelte";
  import { BUSY_HINT } from "../lib/busy";
  import type { Overview } from "../lib/types";

  interface Props {
    page: Page;
    overview: Overview | null;
    onnavigate: (page: Page) => void;
    /** Opens the Undo all flow. The button is disabled while this is unset. */
    onundoall?: () => void;
    undoDisabled?: boolean;
    /** A change is running: navigation and Undo all wait for it. */
    busy?: boolean;
  }

  let { page, overview, onnavigate, onundoall, undoDisabled = false, busy = false }: Props = $props();

  const items: { id: Page; label: string; icon: typeof House }[] = [
    { id: "home", label: "Home", icon: House },
    { id: "remove", label: "Remove", icon: Trash2 },
    { id: "background", label: "Background", icon: Activity },
    { id: "gaming", label: "Gaming", icon: Gamepad2 },
    { id: "cleanup", label: "Cleanup", icon: Sparkles },
  ];

  let list: HTMLUListElement;

  // Up/Down/Home/End move focus between nav items; Tab still works as usual.
  function onkeydown(e: KeyboardEvent) {
    const buttons = Array.from(list.querySelectorAll<HTMLButtonElement>("button"));
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (i < 0) return;
    let next = -1;
    if (e.key === "ArrowDown") next = (i + 1) % buttons.length;
    else if (e.key === "ArrowUp") next = (i - 1 + buttons.length) % buttons.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = buttons.length - 1;
    if (next >= 0) {
      e.preventDefault();
      buttons[next].focus();
    }
  }
</script>

<nav class="sidebar" aria-label="Main">
  <div class="brand">
    <Logo size={24} />
    <span class="name">Featherweight</span>
  </div>

  <ul bind:this={list}>
    {#each items as item (item.id)}
      {@const Icon = item.icon}
      <li>
        <button
          type="button"
          class="item"
          aria-current={page === item.id ? "page" : undefined}
          aria-disabled={busy || undefined}
          title={busy ? BUSY_HINT : undefined}
          onclick={() => !busy && onnavigate(item.id)}
          {onkeydown}
        >
          <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
          <span>{item.label}</span>
        </button>
      </li>
    {/each}
  </ul>

  <div class="foot">
    {#if overview}
      <p class="pc num">
        Build {overview.build} · {overview.isLaptop ? "Laptop" : "Desktop"}
      </p>
    {/if}
    <button
      type="button"
      class="item undo"
      disabled={!onundoall || undoDisabled}
      aria-disabled={busy || undefined}
      title={busy ? BUSY_HINT : undefined}
      onclick={() => !busy && onundoall?.()}
    >
      <RotateCcw size={16} strokeWidth={1.75} aria-hidden="true" />
      <span>Undo all</span>
    </button>
  </div>
</nav>

<style>
  .sidebar {
    width: var(--sidebar-width);
    flex: none;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-2);
    border-right: 1px solid var(--border);
    background: var(--bg);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    padding: 0 var(--space-2);
    margin-bottom: var(--space-2);
  }

  .name {
    font-family: var(--font-display);
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
    letter-spacing: -0.01em;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 34px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--duration) var(--ease),
      color var(--duration) var(--ease);
  }

  .item :global(svg) {
    flex: none;
  }

  .item[aria-disabled="true"]:not([aria-current="page"]) {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .item:not(:disabled):not([aria-disabled="true"]):hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .item:not(:disabled):not([aria-disabled="true"]):active {
    background: var(--surface-3);
  }

  .item[aria-current="page"] {
    background: var(--surface-3);
    color: var(--text);
  }

  .item[aria-current="page"] :global(svg) {
    color: var(--accent);
  }

  .item:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .foot {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .pc {
    padding: 0 var(--space-3);
    font-size: var(--text-xs);
    color: var(--muted);
  }
</style>
