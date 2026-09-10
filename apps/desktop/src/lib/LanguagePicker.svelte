<script lang="ts">
  // The language control, in the topbar beside Account.
  //
  // APP-42 asked for this with two screenshots of competitors' pickers,
  // and the thing worth copying from them is that every language is
  // listed in its own script — 日本語, not "Japanese". A picker that
  // names languages in English is unusable by exactly the person who
  // needs it.
  import Icon from "./Icon.svelte";
  import { tooltip } from "./tooltip";
  import { LOCALES, getLocale, setLocale, t } from "./i18n/index.svelte";

  let open = $state(false);
  let root = $state<HTMLElement | null>(null);
  const current = $derived(getLocale());

  function choose(code: string): void {
    setLocale(code);
    open = false;
  }

  // Close on an outside click or Escape, like the other topbar menus.
  $effect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (root && !root.contains(e.target as Node)) open = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") open = false;
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  });
</script>

<div class="lang" bind:this={root}>
  <button
    class="oa-icon-btn oa-icon-btn--sm"
    class:oa-icon-btn--selected={open}
    onclick={() => (open = !open)}
    use:tooltip={t("Language")}
    aria-label={t("Language")}
    aria-expanded={open}
    aria-haspopup="menu"
  >
    <Icon name="languages" size={15} />
  </button>

  {#if open}
    <ul class="lang__menu" role="menu">
      {#each LOCALES as locale (locale.code)}
        <li role="none">
          <button
            role="menuitemradio"
            aria-checked={locale.code === current}
            class="lang__item"
            class:lang__item--on={locale.code === current}
            onclick={() => choose(locale.code)}
            lang={locale.code}
          >
            {locale.name}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .lang {
    position: relative;
    display: flex;
  }
  .lang__menu {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    z-index: 20;
    min-width: 168px;
    margin: 0;
    padding: var(--space-1);
    list-style: none;
    background: var(--surface-card);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
  }
  .lang__item {
    display: flex;
    align-items: center;
    width: 100%;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    font: var(--type-body);
    font-size: var(--text-sm);
    color: var(--text-strong);
    text-align: left;
    background: transparent;
    border: 0;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .lang__item:hover {
    background: var(--surface-hover);
  }
  .lang__item--on {
    color: var(--accent);
  }
  /* The tick sits before the name rather than after it, so the names
     stay left-aligned in a column whatever their width — a trailing
     mark ragged-rights a list of eight scripts. */
  .lang__item--on::before {
    content: "✓";
    margin-left: calc(var(--space-3) * -1);
    width: var(--space-3);
  }
</style>
