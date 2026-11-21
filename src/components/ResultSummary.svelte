<script lang="ts">
  // What happened after an apply (or undo): outcomes in labelled groups.
  // `itemAction` lets a caller add a per-item control, e.g. "Restore anyway".
  import type { Snippet } from "svelte";
  import { CircleAlert, CircleCheck, CircleMinus } from "lucide-svelte";
  import Modal from "./Modal.svelte";
  import type { ResultGroup, ResultItem } from "../lib/applyFlow";

  interface Props {
    open: boolean;
    title: string;
    summary?: string;
    groups: ResultGroup[];
    onclose: () => void;
    closeLabel?: string;
    returnFocus?: HTMLElement | null;
    itemAction?: Snippet<[ResultItem, ResultGroup]>;
  }

  let {
    open,
    title,
    summary,
    groups,
    onclose,
    closeLabel = "Done",
    returnFocus = null,
    itemAction,
  }: Props = $props();

  const uid = $props.id();
  const icons = { ok: CircleCheck, muted: CircleMinus, bad: CircleAlert };
</script>

<Modal {open} labelledby="{uid}-title" describedby={summary ? `${uid}-sum` : undefined} onescape={onclose} {returnFocus} width={500}>
  <div class="body">
    <h2 id="{uid}-title">{title}</h2>
    {#if summary}
      <p id="{uid}-sum" class="muted">{summary}</p>
    {/if}
    {#each groups as group (group.label)}
      {@const Icon = icons[group.tone]}
      <section class="group {group.tone}" aria-label={group.label}>
        <h3>
          <Icon size={16} strokeWidth={1.75} aria-hidden="true" />
          <span>{group.label}</span>
          <span class="count num">{group.items.length}</span>
        </h3>
        <ul>
          {#each group.items as item, i (i)}
            <li>
              <div class="text">
                <span>{item.title}</span>
                {#if item.detail}<span class="detail">{item.detail}</span>{/if}
              </div>
              {@render itemAction?.(item, group)}
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  </div>
  <div class="actions">
    <button type="button" class="btn btn-ghost" data-autofocus onclick={onclose}>{closeLabel}</button>
  </div>
</Modal>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-5) var(--space-5) var(--space-4);
    overflow-y: auto;
    min-height: 0;
  }

  h3 {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }

  .ok h3 :global(svg) {
    color: var(--safe);
  }
  .bad h3 :global(svg) {
    color: var(--adv);
  }
  .muted h3 :global(svg) {
    color: var(--muted);
  }

  .count {
    color: var(--muted);
    font-weight: var(--weight-regular);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }

  li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-sm);
  }

  li + li {
    border-top: 1px solid var(--border);
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .detail {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    padding: var(--space-4) var(--space-5);
    border-top: 1px solid var(--border);
  }
</style>
