<script lang="ts">
  interface Props {
    label: string;
    /** Already formatted; null shows a placeholder while loading. */
    value: string | null;
    hint?: string;
  }

  let { label, value, hint }: Props = $props();
</script>

<div class="tile">
  <span class="label">{label}</span>
  <span class="value num" aria-label={value === null ? `${label}: not loaded` : undefined}>
    {value ?? "–"}
  </span>
  {#if hint}<span class="hint">{hint}</span>{/if}
</div>

<style>
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-3) var(--space-4);
    min-width: 0;
  }

  .label {
    font-size: var(--text-sm);
    color: var(--muted);
  }

  .value {
    font-family: var(--font-display);
    font-size: var(--text-2xl);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
  }

  .hint {
    font-size: var(--text-xs);
    color: var(--muted);
  }
</style>
