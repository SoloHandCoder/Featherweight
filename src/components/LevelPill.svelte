<script lang="ts">
  import type { Level } from "../lib/types";

  let { level }: { level: Level } = $props();
</script>

<span class="pill {level}">{level === "safe" ? "Safe" : "Advanced"}</span>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    line-height: 1;
    white-space: nowrap;
    flex: none;
  }
  .safe {
    background: var(--safe-bg);
    color: var(--safe);
  }
  .advanced {
    background: var(--adv-bg);
    color: var(--adv);
  }
</style>
