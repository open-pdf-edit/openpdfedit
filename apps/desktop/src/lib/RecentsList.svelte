<script lang="ts">
  // The recent-documents list, in the two places it appears: the start
  // screen, and the History menu in the topbar.
  //
  // One component rather than two copies of the markup, because the
  // second place was added later and the pair would have drifted the
  // first time a row gained anything — the timestamps and the remove
  // button are exactly the sort of detail that gets improved in one
  // list and not the other.
  //
  // The layout differs between the two, and that is the caller's job:
  // this draws the rows, and `--recents-width` sizes them.
  import Icon from "./Icon.svelte";
  import { describeWhen, type RecentDocument } from "./recents";
  import { tooltip } from "./tooltip";

  interface Props {
    entries: RecentDocument[];
    /** The id currently being reopened, if any — the row shows a
     * spinner and the rest go inert, since opening two at once is
     * never what a second click meant. */
    busy: string | null;
    /** Captured once by the caller rather than read per row, so every
     * "20 minutes ago" on screen is measured from the same instant. */
    now: number;
    onOpen: (id: string) => void;
    onForget: (id: string) => void;
    onClear: () => void;
  }

  let { entries, busy, now, onOpen, onForget, onClear }: Props = $props();
</script>

<div class="recents">
  <div class="recents__head">
    <span class="recents__title">Recent</span>
    <button class="recents__clear" onclick={onClear}>Clear</button>
  </div>
  {#each entries as entry (entry.id)}
    <div class="recent">
      <button class="recent__open" onclick={() => onOpen(entry.id)} disabled={busy !== null}>
        <Icon
          name={busy === entry.id ? "loader-circle" : "file-pen"}
          size={15}
          spin={busy === entry.id}
        />
        <span class="recent__name">{entry.name}</span>
        <span class="recent__when">{describeWhen(entry.openedAt, now)}</span>
      </button>
      <button
        class="recent__forget"
        onclick={() => onForget(entry.id)}
        use:tooltip={"Remove from this list"}
        aria-label={`Remove ${entry.name} from the recent list`}
      >
        <Icon name="x" size={13} />
      </button>
    </div>
  {/each}
</div>

<style>
  .recents {
    width: var(--recents-width, min(420px, 100%));
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .recents__head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: 0 var(--space-2) var(--space-1);
  }

  .recents__title {
    font: var(--type-eyebrow);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .recents__clear {
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
    font: var(--type-caption);
    color: var(--text-muted);
  }
  .recents__clear:hover {
    color: var(--text-strong);
  }

  .recent {
    display: flex;
    align-items: stretch;
    border-radius: var(--radius-sm);
  }
  .recent:hover {
    background: var(--surface-hover);
  }

  .recent__open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    background: none;
    border: 0;
    cursor: pointer;
    text-align: left;
    color: inherit;
    font: var(--type-body);
  }
  .recent__open:disabled {
    cursor: default;
  }

  /* The filename is the row: it gets the space, and the age gets
     whatever is left rather than pushing a long name out of view. */
  .recent__name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .recent__when {
    flex: none;
    font: var(--type-caption);
    color: var(--text-muted);
  }

  /* Visible only on hover or focus: a column of ✕ down the side of the
     list competes with the filenames, and removing a row is the rarer
     thing to want by far. */
  .recent__forget {
    flex: none;
    display: flex;
    align-items: center;
    padding: 0 var(--space-2);
    background: none;
    border: 0;
    cursor: pointer;
    color: var(--text-muted);
    opacity: 0;
  }
  .recent:hover .recent__forget,
  .recent__forget:focus-visible {
    opacity: 1;
  }
  .recent__forget:hover {
    color: var(--text-strong);
  }
</style>
