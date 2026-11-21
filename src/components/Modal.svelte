<script lang="ts">
  // Shared modal shell for ConfirmDialog and ResultSummary: role="dialog",
  // aria-modal, focus trap, Esc, and focus returned to the trigger on close.
  // The dialog is moved to <body> and counted in `app.dialogs`, so App can
  // make everything behind it `inert`.
  import { tick, untrack, type Snippet } from "svelte";
  import { closeDialog, openDialog } from "../lib/state.svelte";

  interface Props {
    open: boolean;
    /** Id of the element that names the dialog. */
    labelledby: string;
    describedby?: string;
    /** Esc pressed. */
    onescape: () => void;
    /** Where focus goes on close; defaults to whatever was focused on open. */
    returnFocus?: HTMLElement | null;
    width?: number;
    children: Snippet;
  }

  let { open, labelledby, describedby, onescape, returnFocus = null, width = 460, children }: Props =
    $props();

  let panel: HTMLDivElement | undefined = $state();

  const FOCUSABLE =
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  function focusables(): HTMLElement[] {
    return panel ? Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE)) : [];
  }

  /** Moves the backdrop to <body>, outside the shell that goes inert. */
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  $effect(() => {
    if (!open || !panel) return;
    const el = panel;
    const previous = document.activeElement as HTMLElement | null;
    // untrack: the count is read to bump it, and must not re-run this effect.
    untrack(openDialog);
    const first = el.querySelector<HTMLElement>("[data-autofocus]") ?? focusables()[0] ?? el;
    first.focus();
    return () => {
      const target = returnFocus ?? previous;
      untrack(closeDialog);
      // The shell drops `inert` on the next update; focus the trigger after it.
      void tick().then(() => {
        // Leave focus alone if another dialog has already taken it.
        const active = document.activeElement;
        if (active && active !== document.body && !el.contains(active)) return;
        if (target && target.isConnected) target.focus();
      });
    };
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onescape();
      return;
    }
    if (e.key !== "Tab") return;
    const items = focusables();
    if (items.length === 0) {
      e.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && (document.activeElement === first || document.activeElement === panel)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

{#if open}
  <div class="backdrop" use:portal>
    <div
      bind:this={panel}
      class="panel"
      role="dialog"
      aria-modal="true"
      aria-labelledby={labelledby}
      aria-describedby={describedby}
      tabindex="-1"
      style:width="min({width}px, calc(100vw - 2 * var(--space-6)))"
      {onkeydown}
    >
      {@render children()}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    padding: var(--space-6);
    background: rgb(5 6 9 / 0.66);
  }

  .panel {
    max-height: calc(100vh - 2 * var(--space-6));
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.45);
    outline: none;
  }
</style>
