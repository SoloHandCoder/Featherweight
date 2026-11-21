<script lang="ts">
  // A question with one confirm button, an optional secondary button and a
  // cancel button. Esc cancels. Content beyond `message`/`items` goes in
  // the children snippet.
  import type { Snippet } from "svelte";
  import Modal from "./Modal.svelte";

  interface Props {
    open: boolean;
    title: string;
    message?: string;
    /** Plain list under the message; a pair puts its value (e.g. a size) on the right. */
    items?: (string | { label: string; value: string })[];
    confirmLabel: string;
    cancelLabel?: string;
    /** Optional middle button, e.g. "Continue without one". */
    secondaryLabel?: string;
    onconfirm: () => void;
    oncancel: () => void;
    onsecondary?: () => void;
    /** Put first focus on Cancel, for changes that can't be undone. */
    cautious?: boolean;
    returnFocus?: HTMLElement | null;
    children?: Snippet;
  }

  let {
    open,
    title,
    message,
    items = [],
    confirmLabel,
    cancelLabel = "Cancel",
    secondaryLabel,
    onconfirm,
    oncancel,
    onsecondary,
    cautious = false,
    returnFocus = null,
    children,
  }: Props = $props();

  const uid = $props.id();
</script>

<Modal {open} labelledby="{uid}-title" describedby={message ? `${uid}-msg` : undefined} onescape={oncancel} {returnFocus} width={500}>
  <div class="body">
    <h2 id="{uid}-title">{title}</h2>
    {#if message}
      <p id="{uid}-msg" class="msg">{message}</p>
    {/if}
    {#if items.length > 0}
      <ul class="items">
        {#each items as item, i (i)}
          {#if typeof item === "string"}
            <li>{item}</li>
          {:else}
            <li class="pair"><span>{item.label}</span><span class="num">{item.value}</span></li>
          {/if}
        {/each}
      </ul>
    {/if}
    {@render children?.()}
  </div>
  <div class="actions">
    <button type="button" class="btn btn-ghost" data-autofocus={cautious ? "" : undefined} onclick={oncancel}>
      {cancelLabel}
    </button>
    {#if secondaryLabel && onsecondary}
      <button type="button" class="btn btn-ghost" onclick={onsecondary}>{secondaryLabel}</button>
    {/if}
    <button type="button" class="btn btn-primary" data-autofocus={cautious ? undefined : ""} onclick={onconfirm}>
      {confirmLabel}
    </button>
  </div>
</Modal>

<style>
  .actions .btn {
    white-space: nowrap;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-5) var(--space-5) var(--space-4);
    overflow-y: auto;
    min-height: 0;
  }

  .msg {
    color: var(--muted);
    font-size: var(--text-md);
  }

  .items {
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

  .pair {
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .pair .num {
    color: var(--muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-5);
    border-top: 1px solid var(--border);
  }
</style>
