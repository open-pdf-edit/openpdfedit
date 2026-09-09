<script lang="ts">
  // In-app updates, for the desktop build only.
  //
  // APP-29 was filed as "app内部没有更新按钮" — no update button. There was
  // no button because there was no updater: the app had no way to learn a
  // newer version existed, so the only route was to find the site and
  // download an installer again, which is also what APP-28 is about.
  //
  // Both plugins are imported dynamically. They exist only in the Tauri
  // build; a static import would pull `@tauri-apps/plugin-*` into the web
  // and extension bundles, where the module throws on load rather than
  // politely reporting itself unavailable.
  import Icon from "./Icon.svelte";
  import { tooltip } from "./tooltip";
  import { backendKind } from "./backend";

  /** Build-time, not a runtime probe: this is the constant the web
   * and extension builds branch on, so the whole component and both
   * plugin imports drop out of those bundles rather than shipping as
   * dead code that throws if it ever ran. */
  const isDesktop = backendKind === "tauri";

  /** What the control is currently saying. `idle` is the resting state;
   * everything else is the result of a check — the one the user asked
   * for, or the quiet one after launch.
   *
   * Named `phase` rather than the obvious `state`: a variable called
   * `state` turns every `$state` rune in this file into a store
   * subscription on it, and the errors that produces name neither. */
  type Phase =
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "current" }
    | { kind: "available"; version: string; notes: string }
    | { kind: "downloading"; percent: number | null }
    | { kind: "ready" }
    | { kind: "error"; message: string };

  interface PendingUpdate {
    version: string;
    body?: string;
    downloadAndInstall: (
      onEvent: (event: {
        event: string;
        data?: { contentLength?: number; chunkLength?: number };
      }) => void,
    ) => Promise<void>;
  }

  let phase = $state<Phase>({ kind: "idle" });
  let open = $state(false);
  /** Held between the check and the install: `downloadAndInstall` has to
   * be called on the same object the check returned. */
  let pending: PendingUpdate | null = null;

  /** Whether there is something worth drawing attention to. The dot is
   * the only thing that appears unprompted — a dialog opening itself over
   * a document someone is editing would be worse than the problem. */
  const hasNews = $derived(
    phase.kind === "available" || phase.kind === "ready" || phase.kind === "downloading",
  );
  const busy = $derived(phase.kind === "checking" || phase.kind === "downloading");

  async function check(userAsked: boolean): Promise<void> {
    if (!isDesktop || busy) return;
    phase = { kind: "checking" };
    try {
      const { check: checkForUpdate } = await import("@tauri-apps/plugin-updater");
      const update = await checkForUpdate();
      if (!update) {
        pending = null;
        phase = { kind: "current" };
        return;
      }
      pending = update as unknown as PendingUpdate;
      phase = { kind: "available", version: update.version, notes: update.body ?? "" };
    } catch (e) {
      // A failed check earns no banner when nobody asked for one — the
      // machine may simply be offline, which is the ordinary case for an
      // app whose whole point is working without a network.
      phase = userAsked
        ? { kind: "error", message: e instanceof Error ? e.message : String(e) }
        : { kind: "idle" };
    }
  }

  async function install(): Promise<void> {
    if (!pending) return;
    let total = 0;
    let seen = 0;
    phase = { kind: "downloading", percent: null };
    try {
      await pending.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data?.contentLength ?? 0;
        } else if (event.event === "Progress") {
          seen += event.data?.chunkLength ?? 0;
          phase = {
            kind: "downloading",
            percent: total > 0 ? Math.round((seen / total) * 100) : null,
          };
        }
      });
      phase = { kind: "ready" };
      // Relaunching is the point: an update that only takes effect the
      // next time someone happens to quit reads as having done nothing.
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (e) {
      phase = { kind: "error", message: e instanceof Error ? e.message : String(e) };
    }
  }

  // One quiet check a few seconds after launch, so the dot is already
  // there when someone looks — late enough not to compete with opening a
  // document for the network or the main thread.
  $effect(() => {
    if (!isDesktop) return;
    const timer = setTimeout(() => void check(false), 4000);
    return () => clearTimeout(timer);
  });

  function toggle(): void {
    open = !open;
    if (open && (phase.kind === "idle" || phase.kind === "error")) void check(true);
  }
</script>

{#if isDesktop}
  <div class="update">
    <button
      class="oa-icon-btn oa-icon-btn--sm"
      class:oa-icon-btn--selected={open}
      onclick={toggle}
      use:tooltip={"Check for updates"}
      aria-label="Check for updates"
      aria-expanded={open}
    >
      <Icon name={busy ? "loader-circle" : "rotate-cw"} size={15} spin={busy} />
      {#if hasNews}<span class="update__dot" aria-hidden="true"></span>{/if}
    </button>

    {#if open}
      <div class="update__menu">
        {#if phase.kind === "checking"}
          <p class="update__line">Checking for updates…</p>
        {:else if phase.kind === "current"}
          <p class="update__line">
            <Icon name="circle-check" size={15} /> You're on the latest version.
          </p>
        {:else if phase.kind === "available"}
          <p class="update__title">Version {phase.version} is available</p>
          {#if phase.notes}<p class="update__notes">{phase.notes}</p>{/if}
          <button class="oa-btn oa-btn--primary" onclick={install}>Update and restart</button>
        {:else if phase.kind === "downloading"}
          <p class="update__line">
            {phase.percent === null ? "Downloading…" : `Downloading… ${phase.percent}%`}
          </p>
        {:else if phase.kind === "ready"}
          <p class="update__line">Restarting…</p>
        {:else if phase.kind === "error"}
          <p class="update__line">
            <Icon name="triangle-alert" size={15} />
            {phase.message}
          </p>
          <button class="oa-btn oa-btn--secondary" onclick={() => void check(true)}>
            Try again
          </button>
        {:else}
          <button class="oa-btn oa-btn--secondary" onclick={() => void check(true)}>
            Check for updates
          </button>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .update {
    position: relative;
    display: flex;
  }
  .update__dot {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 6px;
    height: 6px;
    border-radius: var(--radius-full);
    background: var(--accent);
  }
  .update__menu {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 260px;
    padding: var(--space-3);
    background: var(--surface-card);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
  }
  .update__line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font: var(--type-body);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .update__title {
    margin: 0;
    font: var(--type-h3);
    font-size: var(--text-sm);
    color: var(--text-strong);
  }
  .update__notes {
    margin: 0;
    max-height: 8em;
    overflow-y: auto;
    font: var(--type-caption);
    color: var(--text-muted);
    white-space: pre-wrap;
  }
</style>
