<script lang="ts">
  import { onMount } from "svelte";
  import { TriangleAlert } from "lucide-svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import AccountBanner from "./components/AccountBanner.svelte";
  import Home from "./pages/Home.svelte";
  import Section from "./pages/Section.svelte";
  import UndoAll from "./components/UndoAll.svelte";
  import Cleanup from "./pages/Cleanup.svelte";
  import { app, navigate, refresh, type Page } from "./lib/state.svelte";

  const titles: Record<Page, string> = {
    home: "Home",
    remove: "Remove",
    background: "Background",
    gaming: "Gaming",
    cleanup: "Cleanup",
  };

  let undoAll: UndoAll | undefined = $state();

  onMount(() => {
    void refresh();
  });
</script>

<div class="shell">
  <Sidebar
    page={app.page}
    overview={app.overview}
    onnavigate={navigate}
    onundoall={() => undoAll?.start()}
    undoDisabled={!app.backendAvailable || !!app.overview?.journalError}
    busy={app.busy !== null}
  />

  <main class="content" aria-labelledby="page-title">
    <header class="page-head">
      <h1 id="page-title">{titles[app.page]}</h1>
    </header>

    {#if app.error || app.overview?.journalError || app.overview?.accountMessage}
      <div class="notices">
        {#if app.error}
          <div class="notice" role="alert">
            <TriangleAlert size={16} strokeWidth={1.75} aria-hidden="true" />
            <p>{app.error}</p>
            {#if app.backendAvailable}
              <button type="button" class="btn btn-ghost btn-sm" onclick={refresh} disabled={app.loading}>
                {app.loading ? "Retrying" : "Retry"}
              </button>
            {/if}
          </div>
        {/if}
        {#if app.overview?.journalError}
          <div class="notice" role="alert">
            <TriangleAlert size={16} strokeWidth={1.75} aria-hidden="true" />
            <p>
              Changes are paused: the undo record can't be trusted. {app.overview.journalError}
            </p>
          </div>
        {/if}
        <AccountBanner message={app.overview?.accountMessage ?? null} />
      </div>
    {/if}

    {#if app.page === "home"}
      <Home />
    {:else if app.page === "remove" || app.page === "gaming"}
      {#key app.page}<Section section={app.page} />{/key}
    {:else if app.page === "cleanup"}
      <Cleanup />
    {/if}
  </main>
</div>

<UndoAll bind:this={undoAll} />

<style>
  .shell {
    display: flex;
    height: 100%;
    min-height: 0;
  }

  .content {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: var(--space-4) var(--space-6) var(--space-6);
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  .page-head {
    display: flex;
    align-items: center;
    min-height: 32px;
  }

  .notices {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .notice {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    min-height: 40px;
    background: var(--adv-bg);
    border-radius: var(--radius);
    font-size: var(--text-sm);
  }

  .notice p {
    flex: 1;
  }

  .notice :global(svg) {
    flex: none;
    color: var(--adv);
  }

  .notice .btn-ghost {
    border-color: color-mix(in srgb, var(--adv) 35%, transparent);
  }

  .notice .btn-ghost:not(:disabled):hover {
    background: color-mix(in srgb, var(--adv) 12%, transparent);
  }
</style>
