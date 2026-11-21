<script lang="ts">
  // Two tabs: the services and telemetry tweaks (the shared Section page) and
  // the apps that start with Windows. Startup switches apply immediately.
  import { TriangleAlert } from "@lucide/svelte";
  import Section from "./Section.svelte";
  import ApplyDialogs from "../components/ApplyDialogs.svelte";
  import { backend } from "../lib/backend";
  import { app, message, startBusy, stopBusy, BACKEND_UNAVAILABLE } from "../lib/state.svelte";
  import { restorePointStep } from "../lib/applyFlow";
  import type { StartupEntry } from "../lib/types";

  const TABS = [
    { id: "services", label: "Services & telemetry" },
    { id: "startup", label: "Startup apps" },
  ] as const;
  type TabId = (typeof TABS)[number]["id"];

  const uid = $props.id();

  let tab = $state<TabId>("services");
  let entries = $state<StartupEntry[]>([]);
  let loading = $state(false);
  let loaded = $state(false);
  /** Last list failure. A toggle failure is kept apart so the re-list cannot wipe it. */
  let error = $state<string | null>(null);
  let toggleError = $state<string | null>(null);
  /** Id of the entry being switched, for its row's status text. */
  let changing = $state<string | null>(null);
  /** The restore point question is asked once; later toggles reuse the answer. */
  let restoreDone = false;
  let dialogs: ApplyDialogs | undefined = $state();
  let tabButtons: HTMLButtonElement[] = $state([]);

  const busy = $derived(app.busy !== null);
  const canAct = $derived(app.backendAvailable && app.overview !== null && !app.overview.journalError);
  const perUserAllowed = $derived(app.overview?.perUserAllowed ?? true);
  const hasPerUser = $derived(entries.some((e) => e.perUser));

  function onTabKey(e: KeyboardEvent, i: number) {
    let next = i;
    if (e.key === "ArrowRight") next = (i + 1) % TABS.length;
    else if (e.key === "ArrowLeft") next = (i - 1 + TABS.length) % TABS.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = TABS.length - 1;
    else return;
    e.preventDefault();
    tab = TABS[next].id;
    tabButtons[next]?.focus();
  }

  async function load() {
    loading = true;
    try {
      entries = await backend.api.listStartup();
      error = null;
    } catch (e) {
      error = message(e);
    } finally {
      loading = false;
      loaded = true;
    }
  }

  // Load when the tab is first opened. In the dev mock the first refresh
  // swaps the backend in, so wait for it to settle.
  let started = false;
  $effect(() => {
    if (tab !== "startup" || started) return;
    if (!app.backendAvailable) {
      loaded = true;
      return;
    }
    if (app.overview !== null || app.error !== null) {
      started = true;
      void load();
    }
  });

  function isDisabled(e: StartupEntry): boolean {
    return busy || !canAct || (e.perUser && !perUserAllowed);
  }

  async function toggle(entry: StartupEntry) {
    if (isDisabled(entry)) return;
    if (!startBusy({ kind: "apply", label: "Changing startup apps" })) return;
    toggleError = null;
    changing = entry.id;
    const api = backend.api;
    try {
      // The backend refuses startup changes until a restore point was tried.
      if (!restoreDone) {
        const ok = await restorePointStep({
          ensureRestorePoint: () => api.ensureRestorePoint(),
          enableSystemRestore: () => api.enableSystemRestore(),
          confirmRestoreIssue: (outcome, { canEnable }) => dialogs!.askRestore(outcome, canEnable),
        });
        if (!ok) return;
        restoreDone = true;
      }
      await api.setStartupEnabled(entry.id, !entry.enabled);
    } catch (e) {
      toggleError = `Couldn't change ${entry.name}: ${message(e)}`;
    } finally {
      changing = null;
      stopBusy();
    }
    await load();
  }
</script>

<div class="background">
  <div class="tabs" role="tablist" aria-label="Background">
    {#each TABS as t, i (t.id)}
      <button
        type="button"
        role="tab"
        id="{uid}-tab-{t.id}"
        aria-selected={tab === t.id}
        aria-controls="{uid}-panel-{t.id}"
        tabindex={tab === t.id ? 0 : -1}
        bind:this={tabButtons[i]}
        onclick={() => (tab = t.id)}
        onkeydown={(e) => onTabKey(e, i)}
      >{t.label}</button>
    {/each}
  </div>

  <div
    role="tabpanel"
    id="{uid}-panel-services"
    aria-labelledby="{uid}-tab-services"
    hidden={tab !== "services"}
  >
    <Section section="background" />
  </div>

  <div
    role="tabpanel"
    id="{uid}-panel-startup"
    aria-labelledby="{uid}-tab-startup"
    hidden={tab !== "startup"}
    class="startup"
  >
    <p class="intro muted">
      Apps that start when you sign in. Turn one off to stop it launching; you can turn it back on any time.
    </p>

    {#if error || toggleError}
      <div class="notice" role="alert">
        <TriangleAlert size={16} strokeWidth={1.75} aria-hidden="true" />
        <p>{toggleError ?? error}</p>
        {#if app.backendAvailable}
          <button
            type="button"
            class="btn btn-ghost btn-sm"
            aria-disabled={busy || loading || undefined}
            onclick={() => {
              if (busy || loading) return;
              toggleError = null;
              void load();
            }}
          >Retry</button>
        {/if}
      </div>
    {/if}

    {#if app.overview?.journalError}
      <p class="note muted">Changes are paused until the undo record is fixed, so these switches are locked.</p>
    {/if}

    {#if hasPerUser && !perUserAllowed}
      <p class="note muted">
        Featherweight is running as a different account than the one signed in, so apps that start for only that account can't be changed here.
      </p>
    {/if}

    <ul class="list" aria-label="Startup apps" aria-busy={loading}>
      {#if !app.backendAvailable}
        <li class="empty muted">{BACKEND_UNAVAILABLE}</li>
      {:else if !loaded}
        {#each [0, 1, 2, 3] as i (i)}
          <li class="row skeleton" aria-hidden="true">
            <span class="ph sw"></span>
            <span class="ph label" style:width="{[30, 40, 26, 36][i]}%"></span>
          </li>
        {/each}
        <li class="sr-only">Loading startup apps</li>
      {:else if entries.length === 0 && !error}
        <li class="empty muted">Nothing starts with Windows.</li>
      {:else}
        {#each entries as e (e.id)}
          <li class="row" class:off={!e.enabled}>
            <button
              type="button"
              role="switch"
              aria-checked={e.enabled}
              aria-label={e.name}
              aria-disabled={isDisabled(e)}
              title={e.perUser && !perUserAllowed ? "Only the signed-in account can change this" : undefined}
              class="switch"
              onclick={() => toggle(e)}
            ><span class="knob"></span></button>
            <span class="text">
              <span class="name">
                {e.name}
                {#if !e.perUser}<span class="tag">All users</span>{/if}
                {#if changing === e.id}<span class="muted status" role="status">Saving</span>{/if}
              </span>
              <span class="command muted" title={e.command}>{e.command}</span>
            </span>
          </li>
        {/each}
      {/if}
    </ul>
  </div>
</div>

<ApplyDialogs bind:this={dialogs} tweaks={[]} />

<style>
  .background {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-height: 0;
  }

  [role="tabpanel"][hidden] {
    display: none;
  }

  .tabs {
    display: flex;
    gap: var(--space-1);
    border-bottom: 1px solid var(--border);
  }

  [role="tab"] {
    padding: var(--space-2) var(--space-4);
    margin-bottom: -1px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--muted);
    font-size: var(--text-md);
    cursor: pointer;
    transition: color var(--duration) var(--ease);
  }

  [role="tab"]:hover {
    color: var(--text);
  }

  [role="tab"][aria-selected="true"] {
    color: var(--text);
    border-bottom-color: var(--text);
    font-weight: var(--weight-medium);
  }

  .startup {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .startup[hidden] {
    display: none;
  }

  .intro {
    font-size: var(--text-md);
  }

  .note {
    font-size: var(--text-sm);
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

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    overflow: hidden;
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 56px;
    padding: var(--space-2) var(--space-4);
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .row:not(.skeleton):hover {
    background: var(--surface-2);
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-md);
  }

  .row.off .name {
    color: var(--muted);
  }

  .status {
    font-size: var(--text-xs);
  }

  .tag {
    flex: none;
    padding: 1px var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--surface-3);
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .command {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty {
    padding: var(--space-5) var(--space-4);
  }

  .ph {
    display: block;
    height: 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-3);
  }

  .ph.sw {
    width: 32px;
    height: 18px;
    border-radius: var(--radius-pill);
  }
</style>
