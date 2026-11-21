<script lang="ts">
  import { UserRound } from "@lucide/svelte";

  /** `overview.accountMessage`; nothing renders when it is null. */
  let { message }: { message: string | null } = $props();
</script>

{#if message}
  <div class="banner" role="status">
    <UserRound size={16} strokeWidth={1.75} aria-hidden="true" />
    <p>{message}</p>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    background: var(--surface-2);
    border-radius: var(--radius);
    font-size: var(--text-sm);
    line-height: var(--leading);
  }
  .banner :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--muted);
  }
</style>
