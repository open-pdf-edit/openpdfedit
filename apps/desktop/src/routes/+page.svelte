<script lang="ts">
  import { backend, backendKind, isPasswordRequired } from "$lib/backend";
  import type {
    AnnotationSummaryDto,
    CompareReportDto,
    FormFieldDto,
    OpenedDocument,
    OutlineEntryDto,
    SearchHitDto,
    SignatureInfoDto,
    TextRunDto,
    ImagePlacementDto,
  } from "$lib/backend/types";
  import Viewer from "$lib/Viewer.svelte";
  import CommentsPanel from "$lib/CommentsPanel.svelte";
  import PagesPanel from "$lib/PagesPanel.svelte";
  import FormsPanel from "$lib/FormsPanel.svelte";
  import SignaturesPanel from "$lib/SignaturesPanel.svelte";
  import SearchPanel from "$lib/SearchPanel.svelte";
  import OutlinePanel from "$lib/OutlinePanel.svelte";
  import SignaturePad from "$lib/SignaturePad.svelte";
  import { savedSignatures, addSignature } from "$lib/signatures.svelte";
  import DialogHost from "$lib/DialogHost.svelte";
  import AccountPanel from "$lib/AccountPanel.svelte";
  import WatermarkPanel from "$lib/WatermarkPanel.svelte";
  import SupporterGate, { type GateState } from "$lib/SupporterGate.svelte";
  import { SUPPORTER_TOOLS_ARE_PREMIUM, supporterState, unlockSupporter } from "$lib/openapps";
  import { getClient, onChange } from "@openapps/ui";
  import NumberingPanel from "$lib/NumberingPanel.svelte";
  import EncryptPanel from "$lib/EncryptPanel.svelte";
  import type { EncryptChoices, NumberPagesChoices, WatermarkChoices } from "$lib/backend/types";
  import { showAlert, showChoice, showConfirm, showPrompt } from "$lib/dialog.svelte";
  import ToastHost from "$lib/ToastHost.svelte";
  import { showToast } from "$lib/toast.svelte";
  import { TOOL_GROUPS, type Tool } from "$lib/tools";
  import { untrack } from "svelte";
  import type { AnnotationPayload } from "$lib/PdfPage.svelte";
  import BrandMark from "$lib/BrandMark.svelte";
  import ZoomControl from "$lib/ZoomControl.svelte";
  import InstallPrompt from "$lib/InstallPrompt.svelte";
  import Icon from "$lib/Icon.svelte";
  import { tooltip } from "$lib/tooltip";
  import { describeWhen, type RecentDocument } from "$lib/recents";

  // Milestone M2 scope (PLAN.md): annotations (markup tools + comments
  // panel), on top of M1's open/scroll/zoom viewer. Pixels come from the
  // `tile://` custom URI scheme (openpdfedit-engine's dedicated render
  // thread + LRU cache, served by apps/desktop/src-tauri/src/lib.rs) as
  // a raw RGBA fetch response per visible page, not JSON IPC.
  //
  // Annotation writes go through `add_annotation_cmd`, which returns a
  // *new* handle every time (the underlying file changed and the render
  // side reopens it — see annotations.rs's module doc) — `doc` is
  // reassigned wholesale on every successful edit rather than patched,
  // so the viewer always renders through the current handle.

  const MIN_ZOOM = 0.25;
  const MAX_ZOOM = 4;
  const ZOOM_STEP = 1.25;
  const PRESET_COLORS: { label: string; value: [number, number, number] }[] = [
    { label: "Yellow", value: [1, 0.92, 0.23] },
    { label: "Red", value: [0.96, 0.26, 0.21] },
    { label: "Black", value: [0, 0, 0] },
  ];

  /** One open document. `doc` and `filePath` below stay the *active*
   * tab's values so every existing handler keeps working unchanged;
   * switching tabs swaps them and re-derives the rest.
   *
   * Only the cheap view state lives here. Annotations, form fields,
   * signatures and the outline are re-fetched on switch rather than
   * cached per tab: they're a handful of fast backend reads, and caching
   * them would mean invalidating four things on every mutation in every
   * inactive tab — a much easier thing to get subtly wrong than to
   * simply ask again. */
  interface Tab {
    doc: OpenedDocument;
    filePath: string;
    zoom: number;
    currentPage: number;
  }

  let tabs = $state<Tab[]>([]);
  let activeTabIndex = $state(-1);

  let filePath = $state<string | null>(null);
  let doc = $state<OpenedDocument | null>(null);
  let error = $state<string | null>(null);

  /** Whether saving this document produces a download rather than
   * writing back over the file that was opened. Always false on the
   * desktop; true in a browser without the File System Access API. The
   * Save control says which, so it never promises something it can't do.
   * `doc` is read so this re-evaluates when the active tab changes. */
  const savesByDownloading = $derived(
    doc ? backend.savesByDownloading(doc.handle) : false,
  );

  // ---- Print ----
  /** Whether this build can print at all. A capability of the backend
   * rather than of the build kind, so the desktop picks the control up
   * on its own once it grows a native print path — see
   * `Backend.canPrint`. */
  const canPrint = backend.canPrint();
  let printBusy = $state(false);

  async function handlePrint() {
    if (!doc || printBusy) return;
    error = null;
    printBusy = true;
    try {
      await backend.printDocument(doc.handle);
    } catch (e) {
      error = formatError(e);
    } finally {
      printBusy = false;
    }
  }

  // ---- Protect with a password ----
  let showEncrypt = $state(false);
  let encryptBusy = $state(false);

  async function handleEncrypt(choices: EncryptChoices) {
    if (!doc) return;
    error = null;
    encryptBusy = true;
    try {
      // Export, not mutation: the open document is untouched, so there's
      // no handle to swap and nothing to refresh.
      const result = await backend.encryptDocument(doc.handle, choices);
      if (!result) return;
      showEncrypt = false;
      showToast(
        "Saved a password-protected copy. The document you're editing is unchanged.",
        { title: "Protected" },
      );
    } catch (e) {
      error = formatError(e);
    } finally {
      encryptBusy = false;
    }
  }

  // ---- Page numbers / Bates ----
  let showNumbering = $state(false);

  async function handleNumberPages(choices: NumberPagesChoices) {
    if (!doc || mutationBusy) return;
    const handle = doc.handle;
    error = null;
    mutationBusy = true;
    try {
      doc = await backend.numberPages(handle, choices);
      showNumbering = false;
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
      showToast("Pages numbered — ⌘Z undoes it.", { title: "Number pages" });
    } catch (e) {
      // The panel stays open on failure: an unsupported character in a
      // Bates prefix is something to correct in place.
      error = formatError(e);
    } finally {
      mutationBusy = false;
    }
  }

  // ---- Flatten ----
  let flattenBusy = $state(false);

  async function handleFlatten() {
    if (!doc || mutationBusy) return;
    const handle = doc.handle;
    const hasFields = formFields.length > 0;

    const confirmed = await showConfirm(
      hasFields
        ? "Markup and filled-in form values become part of the page. Afterwards they can't be " +
            "edited, moved or removed, and the form can't be filled in again.\n\n" +
            "This is what you want before sending a signed or marked-up document to someone else."
        : "Markup becomes part of the page. Afterwards it can't be edited, moved or removed.\n\n" +
            "This is what you want before sending a marked-up document to someone else.",
      { title: "Flatten", confirmLabel: hasFields ? "Flatten markup and fields" : "Flatten markup" },
    );
    if (!confirmed) return;

    error = null;
    flattenBusy = true;
    mutationBusy = true;
    try {
      const result = await backend.flattenDocument({
        handle,
        annotations: true,
        formFields: hasFields,
      });
      doc = result.document;
      await Promise.all([refreshAnnotations(), refreshFormFields(), refreshSignatures()]);
      showToast(
        result.flattened === 0
          ? "Nothing to flatten — this document has no markup with a visible appearance."
          : `Flattened ${result.flattened} item${result.flattened === 1 ? "" : "s"}` +
              (result.skipped > 0
                ? `, left ${result.skipped} interactive (links and hidden markup)`
                : "") +
              ". ⌘Z undoes it.",
        { title: "Flatten" },
      );
    } catch (e) {
      error = formatError(e);
    } finally {
      flattenBusy = false;
      mutationBusy = false;
    }
  }

  // ---- Recent documents ----
  //
  // Refreshed whenever the landing screen comes back rather than only
  // on mount: closing a document returns you here, and the file you
  // just had open should be at the top of the list when it does.
  let recents = $state<RecentDocument[]>([]);
  let recentBusy = $state<string | null>(null);
  // Captured once per refresh rather than read per row, so every "20
  // minutes ago" on screen is measured from the same instant.
  let now = $state(Date.now());

  async function refreshRecents() {
    now = Date.now();
    recents = await backend.recentDocuments();
  }

  $effect(() => {
    // Reading `doc` subscribes this to opening and closing.
    if (doc) return;
    void refreshRecents();
  });

  async function handleOpenRecent(id: string) {
    if (recentBusy) return;
    recentBusy = id;
    error = null;
    try {
      // Already open: the same check `openInNewTab` makes, and the same
      // answer — go to it rather than opening a second copy.
      const existing = tabs.findIndex((tab) => tab.filePath === id);
      if (existing !== -1) {
        await switchToTab(existing);
        return;
      }

      const opened = await backend.openRecent(id);
      if (!opened) {
        // Gone, or permission refused. `openRecent` has already dropped
        // the entry if the file itself is gone, so the refreshed list
        // is the answer — no banner for something this ordinary.
        await refreshRecents();
        showToast("Couldn't reopen that one — it may have been moved, renamed or deleted.", {
          title: "Recent",
        });
        return;
      }
      await adoptAsNewTab(opened, opened.file_path);
    } catch (e) {
      error = formatError(e);
    } finally {
      recentBusy = null;
    }
  }

  async function handleForgetRecent(id: string) {
    await backend.forgetRecent(id);
    await refreshRecents();
  }

  async function handleClearRecents() {
    await backend.clearRecents();
    await refreshRecents();
  }

  // ---- Remove markup ----
  let unmarkBusy = $state(false);

  async function handleRemoveMarkup() {
    if (!doc || mutationBusy) return;
    const handle = doc.handle;

    const confirmed = await showConfirm(
      "Highlights, notes, ink and stamps are deleted from every page — including pen and " +
        "highlighter that was already flattened into the page by whatever app exported it, " +
        "which nothing else here can reach.\n\n" +
        "The document underneath is left alone, and ⌘Z undoes it. This tidies a document " +
        "rather than sanitising it: use Redact if the markup itself must not be recoverable.",
      { title: "Remove markup", confirmLabel: "Remove markup" },
    );
    if (!confirmed) return;

    error = null;
    unmarkBusy = true;
    mutationBusy = true;
    try {
      const result = await backend.removeMarkup({ handle });
      doc = result.document;
      await Promise.all([refreshAnnotations(), refreshFormFields(), refreshSignatures()]);

      const parts: string[] = [];
      if (result.annotations > 0) {
        parts.push(`${result.annotations} annotation${result.annotations === 1 ? "" : "s"}`);
      }
      if (result.layers > 0) {
        parts.push(`${result.layers} flattened layer${result.layers === 1 ? "" : "s"}`);
      }
      showToast(
        parts.length === 0
          ? "Nothing to remove — no annotations, and no flattened pen layer this can identify."
          : `Removed ${parts.join(" and ")}. ⌘Z undoes it.`,
        { title: "Remove markup" },
      );
    } catch (e) {
      error = formatError(e);
    } finally {
      unmarkBusy = false;
      mutationBusy = false;
    }
  }

  // ---- Contents (bookmarks) ----
  let showOutline = $state(false);
  let outline = $state<OutlineEntryDto[]>([]);
  let outlineLoading = $state(false);
  let currentPage = $state(0);
  /** A page-jump request for the viewer. The nonce is what makes clicking
   * the same bookmark twice scroll again. */
  let scrollToPage = $state<{ pageIndex: number; nonce: number } | null>(null);
  let scrollNonce = 0;

  async function refreshOutline() {
    if (!doc) {
      outline = [];
      return;
    }
    outlineLoading = true;
    try {
      outline = await backend.documentOutline(doc.handle);
    } catch {
      // A document whose outline won't read is not a document that
      // failed to open — show no bookmarks rather than an error banner.
      outline = [];
    } finally {
      outlineLoading = false;
    }
  }

  function goToPage(pageIndex: number) {
    scrollToPage = { pageIndex, nonce: ++scrollNonce };
  }

  // ---- Find in document ----
  // Hidden until asked for (⌘F): this is a low-frequency, task-driven
  // tool, and a permanently-parked search box would eat topbar width the
  // markup tools use every session.
  let showSearch = $state(false);
  let showSearchResults = $state(false);
  let searchQuery = $state("");
  let searchMatchCase = $state(false);
  let searchWholeWord = $state(false);
  let searchHits = $state<SearchHitDto[]>([]);
  let searchActiveIndex = $state(-1);
  let searchBusy = $state(false);
  let searchTruncated = $state(false);
  /** Whether the current query has actually run, so the results panel can
   * tell "nothing typed yet" apart from "no matches". */
  let searchRan = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  /** Debounce before a keystroke becomes a query. On the desktop, search
   * runs on the shared render thread; in the extension it runs on the
   * page's only thread. Firing per keystroke on a long document queues
   * full-document scans ahead of the tile renders the viewer needs. */
  const SEARCH_DEBOUNCE_MS = 200;
  let searchDebounce: ReturnType<typeof setTimeout> | undefined;
  /** Guards against an earlier, slower query overwriting a later one — a
   * real risk here, since a one-character query takes far longer than a
   * specific one. */
  let searchGeneration = 0;

  async function runSearch() {
    const handle = doc?.handle;
    const query = searchQuery;
    const generation = ++searchGeneration;

    if (!handle || query.trim() === "") {
      searchHits = [];
      searchActiveIndex = -1;
      searchTruncated = false;
      searchRan = false;
      searchBusy = false;
      return;
    }

    searchBusy = true;
    try {
      const results = await backend.searchDocument({
        handle,
        query,
        matchCase: searchMatchCase,
        wholeWord: searchWholeWord,
      });
      if (generation !== searchGeneration) return;
      searchHits = results.hits;
      searchTruncated = results.truncated;
      searchRan = true;
      // Jump straight to the first match: the point of typing a query is
      // to be taken to it, not to then have to press Enter as well.
      searchActiveIndex = results.hits.length > 0 ? 0 : -1;
    } catch (e) {
      if (generation !== searchGeneration) return;
      error = formatError(e);
      searchHits = [];
      searchActiveIndex = -1;
      searchRan = true;
    } finally {
      if (generation === searchGeneration) searchBusy = false;
    }
  }

  function scheduleSearch() {
    clearTimeout(searchDebounce);
    searchDebounce = setTimeout(runSearch, SEARCH_DEBOUNCE_MS);
  }

  function openSearch() {
    if (!doc) return;
    showSearch = true;
    // Focus once the bar has rendered. Selecting the existing text makes
    // reopening behave like every other find box: type to replace, or
    // press Enter to step through what it already found.
    queueMicrotask(() => {
      searchInputEl?.focus();
      searchInputEl?.select();
    });
  }

  function closeSearch() {
    clearTimeout(searchDebounce);
    searchGeneration++;
    showSearch = false;
    searchHits = [];
    searchActiveIndex = -1;
    searchTruncated = false;
    searchRan = false;
    searchBusy = false;
  }

  function stepSearch(delta: number) {
    if (searchHits.length === 0) return;
    // Wrap in both directions, the way every find bar does.
    searchActiveIndex = (searchActiveIndex + delta + searchHits.length) % searchHits.length;
  }

  /** The handle the currently-displayed hits were found against. */
  let lastSearchedHandle: number | null = null;

  // Every write rotates the render handle, so a changed handle is exactly
  // "this document's bytes are different now" — after an edit, an undo, a
  // page reorder. Character indices and page geometry both move, so stale
  // hits would point at the wrong text, and quietly highlighting the
  // wrong words is worse than briefly showing none. `untrack` keeps this
  // firing on the handle alone: the query has its own debounce, and
  // re-running it here too would double every search.
  $effect(() => {
    const handle = doc?.handle ?? null;
    untrack(() => {
      if (handle === lastSearchedHandle) return;
      lastSearchedHandle = handle;
      if (handle === null) {
        closeSearch();
        outline = [];
        return;
      }
      // Bookmarks are re-read for the same reason search hits are: a
      // page delete or reorder moves every destination after it, and a
      // contents list that jumps to the wrong page is worse than none.
      refreshOutline();
      if (!showSearch || searchQuery.trim() === "") return;
      runSearch();
    });
  });
  let zoom = $state(1);

  /* Below this the layout switches to the phone arrangement — the same
   * number as the media query at the bottom of this file. Kept in both
   * places rather than plumbed through, because CSS cannot read a
   * constant and a wrong guess here only mis-sizes the first render. */
  const PHONE_MAX_PX = 720;

  /** The zoom that makes a page fill the width available to it.
   *
   * A Letter page at 100% is 816 CSS px — twice a phone's width, so a
   * document opened at zoom 1 shows its left half and nothing suggests
   * the rest is there. Desktop keeps 100%, where a page fits and a
   * predictable scale is worth more than filling the window. */
  function fitWidthZoom(pageWidthPt: number): number {
    const available = window.innerWidth - 16; // the viewer's own padding
    const atFullSize = pageWidthPt * (96 / 72);
    return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, available / atFullSize));
  }

  /** Which document the fitted zoom was last applied for, so it happens
   * once per document rather than on every render. */
  let fittedFor: OpenedDocument | null = null;

  /* Fit a newly opened document to a phone's width.
   *
   * An effect rather than a call at the open site, because two earlier
   * attempts there were both wrong about *when* they ran — one measured
   * the document being replaced, the other was undone by the tab's own
   * stored zoom. Reacting to `doc` cannot be early or late by
   * construction.
   *
   * Only for a document not yet fitted, so someone who zooms keeps their
   * zoom, and only on a phone: on a desktop a page fits at 100% and a
   * predictable scale is worth more than filling the window. */
  $effect(() => {
    const current = doc;
    if (!current || current === fittedFor) return;
    fittedFor = current;
    if (typeof window === "undefined" || window.innerWidth > PHONE_MAX_PX) return;
    const first = current.page_sizes?.[0];
    if (first) zoom = fitWidthZoom(first.width);
  });

  // `doc` is reassigned wholesale by every mutating handler (the handle
  // rotates on each write), and `zoom`/`currentPage` change as the user
  // works. Mirroring them back into the active tab here means no handler
  // has to know tabs exist.
  $effect(() => {
    const active = tabs[activeTabIndex];
    if (!active || !doc) return;
    if (
      active.doc !== doc ||
      active.zoom !== zoom ||
      active.currentPage !== currentPage ||
      active.filePath !== filePath
    ) {
      tabs[activeTabIndex] = {
        doc,
        filePath: filePath ?? active.filePath,
        zoom,
        currentPage,
      };
    }
  });

  /** Makes `index` the active tab, restoring its view state and
   * re-deriving everything else. */
  async function switchToTab(index: number) {
    if (index < 0 || index >= tabs.length || index === activeTabIndex) return;
    const target = tabs[index];
    activeTabIndex = index;
    doc = target.doc;
    filePath = target.filePath;
    zoom = target.zoom;
    currentPage = target.currentPage;
    activeTool = "select";
    closeSearch();
    await Promise.all([
      refreshAnnotations(),
      refreshFormFields(),
      refreshSignatures(),
      refreshOutline(),
    ]);
  }

  /** Closes one tab, prompting first if it has unsaved edits. */
  async function closeTab(index: number) {
    const tab = tabs[index];
    if (!tab) return;
    if (tab.doc.is_dirty) {
      const discard = await showConfirm(
        `"${tab.filePath.split(/[/\\]/).pop()}" has unsaved changes. Close it anyway?`,
        { title: "Unsaved changes", confirmLabel: "Discard and close", destructive: true },
      );
      if (!discard) return;
    }

    try {
      await backend.closeDocument(tab.doc.handle);
    } catch (e) {
      // The tab is going away regardless; failing to release the
      // backend's copy is a logged leak, not a reason to keep a document
      // the user asked to close.
      console.error(`closeDocument failed for handle ${tab.doc.handle}:`, e);
    }

    tabs.splice(index, 1);
    if (tabs.length === 0) {
      activeTabIndex = -1;
      doc = null;
      filePath = null;
      outline = [];
      closeSearch();
      return;
    }
    // Fall back to the neighbour on the left, which is where the eye
    // already is after a close.
    const next = Math.min(index, tabs.length - 1);
    activeTabIndex = -1; // force switchToTab to run even for the same index
    await switchToTab(next);
  }
  let activeTool = $state<Tool>("select");
  /** Set when a text field has just been drawn, so the page can put the
   * cursor in it once it renders. */
  let focusField = $state<{ name: string; nonce: number } | null>(null);
  // Plain (non-reactive) mutable variable, deliberately: it only needs to
  // remember what the *previous* $effect run saw, not to trigger runs of
  // its own. Seeded to activeTool's own initial value below.
  let previousTool: Tool = "select";

  // The Shift-to-constrain-axis gesture on the move tools (see
  // PdfPage.svelte's onPointerMove) has no other affordance pointing at
  // it, so remind the user once each time they switch into one of these
  // tools — not on every keystroke or drag, just on the tool becoming
  // active, and non-blocking so it never interrupts the drag itself.
  $effect(() => {
    const tool = activeTool;
    if (tool !== previousTool) {
      if (tool === "moveText" || tool === "moveImage") {
        showToast("Hold Shift while dragging to move only horizontally or vertically.", {
          title: tool === "moveText" ? "Move text" : "Move image",
        });
      } else if (tool === "signature") {
        // The Signatures panel is where you pick which saved signature
        // to place (or draw a new one) — surface it automatically
        // rather than making the user go find the toggle button too.
        showSignatures = true;
        showToast(
          savedSignatures().length === 0
            ? "Draw a signature, then drag on the page to place it."
            : "Pick a saved signature below, then drag on the page to place it.",
          { title: "Signature" },
        );
      }
    }
    previousTool = tool;
  });
  let color = $state<[number, number, number]>(PRESET_COLORS[0].value);
  let showComments = $state(false);
  let showPages = $state(false);
  let showForms = $state(false);
  let showSignatures = $state(false);
  let showSignaturePad = $state(false);
  let showAccount = $state(false);
  let showWatermark = $state(false);
  /** Phone only: whether the document-tools sheet is open. On a desktop
   * those tools sit in the topbar and this is never read — see the
   * `.topbar__group--overflow` rules. */
  let moreOpen = $state(false);
  // ---- The Supporter gate ----
  //
  // Two tools are paid: the watermark and OCR. One unlock covers both —
  // the entitlement is Supporter, not "watermark" — so someone who
  // bought it for one gets the other, and nobody is asked to pay twice
  // for the same tier. Clicking either tool is the only place any of
  // this is checked; nothing else in the app asks about an account.
  let gateState = $state<GateState>({ kind: "hidden" });
  // Once this session has established the tool is unlocked, stop asking:
  // a round trip on every click would make an unlocked tool feel slower
  // than a locked one. Deliberately not persisted — the server's answer
  // is the authority, and a cached "yes" sitting on disk would be a
  // thing to forge.
  let supporterUnlocked = $state(false);
  let gateCheckInFlight = false;

  function accessToken(): string | undefined {
    return getClient()?.session?.accessToken;
  }

  /** Which tool the gate is standing in front of, so unlocking resumes
   * the thing that was clicked rather than dropping the user back where
   * they started. */
  let gatedTool = $state<"watermark" | "ocr">("watermark");

  function runGatedTool(): void {
    if (gatedTool === "watermark") showWatermark = true;
    else void runOcr();
  }

  /**
   * Make the stored session usable again, if it can be.
   *
   * An access token is short-lived; a refresh token is not. After a
   * reload the app has a session that looks fine — `isLoggedIn` is true,
   * there is a token — and whose access token the server will refuse.
   * The SDK handles that automatically, but only for requests it makes
   * itself: it refreshes on a 401 and retries. The entitlement check and
   * the unlock are plain `fetch` calls with a bearer header, so they got
   * the 401 and reported it as "not unlocked" and "couldn't unlock".
   *
   * That is the whole of the reported bug: after a hard refresh the app
   * looked signed out, unlocking a feature already paid for failed, and
   * opening the account panel fixed it — because the panel asks for a
   * balance, which goes through the SDK, which refreshes.
   *
   * So this asks for the balance first. Any request through the client
   * would do; the balance is the cheapest, and the panel wants it
   * anyway. Returns false only when the refresh token is dead too, which
   * is the one case that really is "signed out".
   */
  /** Whether there is a session, for the one place that says so without
   * being opened.
   *
   * "It appears that my openpdfedit is not logged in" was half the
   * reported problem, and it was true of the interface even when it was
   * false of the session: nothing on screen said either way until the
   * account panel was opened. A dot on the account button does. */
  let signedIn = $state(false);

  $effect(() => {
    const update = () => {
      signedIn = getClient()?.isLoggedIn ?? false;
    };
    update();
    // A sign-in in the account panel, or in another tab.
    const stop = onChange(update);
    return stop;
  });

  $effect(() => {
    // Get the token refreshed before anything needs it, rather than at
    // the moment someone clicks a paid tool and is told they are not
    // signed in. Fire and forget: nothing on screen waits for it, and a
    // failure here is indistinguishable from being offline.
    if (!getClient()?.isLoggedIn) return;
    void ensureUsableSession();
  });

  async function ensureUsableSession(): Promise<boolean> {
    const client = getClient();
    if (!client?.isLoggedIn) return false;
    try {
      await client.credits.balance();
      return true;
    } catch {
      return false;
    }
  }

  /** A paid tool's click. Runs it when it's available and shows the gate
   * only when it isn't, so the gate never stands in front of someone who
   * has already paid. */
  async function requireSupporter(tool: "watermark" | "ocr"): Promise<void> {
    gatedTool = tool;
    if (!SUPPORTER_TOOLS_ARE_PREMIUM || supporterUnlocked) {
      runGatedTool();
      return;
    }
    if (gateCheckInFlight) return;
    gateCheckInFlight = true;
    try {
      gateState = { kind: "checking" };
      if (!getClient()?.isLoggedIn) {
        gateState = { kind: "signed-out" };
        return;
      }

      let state = await supporterState(accessToken());
      if (state === "unauthorized") {
        // Stale token. Refresh it and ask once more — and only call it
        // signed out if that fails, rather than on the first refusal.
        state = (await ensureUsableSession())
          ? await supporterState(accessToken())
          : "unauthorized";
      }

      if (state === "unlocked") {
        supporterUnlocked = true;
        gateState = { kind: "hidden" };
        runGatedTool();
        return;
      }
      gateState = state === "unauthorized" ? { kind: "signed-out" } : { kind: "locked" };
    } finally {
      gateCheckInFlight = false;
    }
  }

  // --- Markdown ---
  //
  // A PDF's text as something editable. The conversion is local — see
  // `openpdfedit_session::markdown` — and it converts the document as it
  // stands, so a scan that has been OCR'd converts and one that has not
  // produces nothing, which is worth saying out loud rather than
  // handing over an empty file.

  let markdownBusy = $state(false);

  /** The Obsidian vault, if one has been chosen. A vault is a folder of
   * `.md` files and nothing more, so "export to Obsidian" is "write the
   * file in there" — the difference from an ordinary save is only that
   * the folder is remembered. */
  const VAULT_KEY = "openpdfedit.markdown.vault";

  function rememberedVault(): { key: string; name: string } | null {
    try {
      const stored = localStorage.getItem(VAULT_KEY);
      return stored ? (JSON.parse(stored) as { key: string; name: string }) : null;
    } catch {
      return null;
    }
  }

  function rememberVault(vault: { key: string; name: string } | null): void {
    try {
      if (vault) localStorage.setItem(VAULT_KEY, JSON.stringify(vault));
      else localStorage.removeItem(VAULT_KEY);
    } catch {
      /* choosing the folder again next time is the whole cost */
    }
  }

  /** report.pdf -> report.md, and anything without an extension keeps
   * its name. */
  function markdownNameFor(path: string): string {
    const base = path.split(/[\\/]/).pop() || "document.pdf";
    return `${base.replace(/\.pdf$/i, "")}.md`;
  }

  async function handleExportMarkdown(): Promise<void> {
    if (!doc || !filePath || markdownBusy) return;
    const fileName = markdownNameFor(filePath);
    const vault = rememberedVault();

    const choices = [{ value: "file", label: "Save as a Markdown file" }];
    if (backend.supportsVault()) {
      if (vault) choices.push({ value: "vault", label: `Save to ${vault.name}` });
      choices.push({
        value: "pick",
        label: vault ? "Choose a different folder…" : "Save to an Obsidian vault…",
      });
    }

    // With nowhere else to put it, there is nothing to ask about.
    const chosen =
      choices.length === 1
        ? "file"
        : await showChoice("Where should the Markdown go?", {
            title: "Export as Markdown",
            choices,
            defaultValue: vault ? "vault" : "file",
            confirmLabel: "Export",
          });
    if (chosen === null) return;

    let target: string | null = null;
    if (chosen === "vault") target = vault?.key ?? null;
    if (chosen === "pick") {
      const picked = await backend.pickVault();
      if (!picked) return;
      rememberVault(picked);
      target = picked.key;
    }

    markdownBusy = true;
    error = null;
    try {
      const result = await backend.exportMarkdown({ handle: doc.handle, fileName, vault: target });
      if (result.path === null && result.characters === 0) return; // cancelled
      if (result.characters === 0) {
        await showAlert(
          "There was no text to convert. This looks like a scan — run OCR on it first, then export again.",
          "Nothing to export",
        );
        return;
      }
      showToast(
        result.path ? `Saved ${result.path}.` : `Downloaded ${fileName}.`,
        { title: "Markdown" },
      );
    } catch (e) {
      error = formatError(e);
      // A folder that has been moved, renamed or had its permission
      // revoked cannot be written to again — forget it rather than
      // offering it every time from now on.
      if (chosen === "vault") rememberVault(null);
    } finally {
      markdownBusy = false;
    }
  }

  // ---- Export as plain text ----
  let textBusy = $state(false);

  /** No vault question, unlike Markdown. The vault exists because
   * Obsidian reads Markdown; a `.txt` dropped into one is a file
   * Obsidian will not treat as a note, so offering the choice would
   * only be a way to get it wrong. */
  async function handleExportText(): Promise<void> {
    if (!doc || !filePath || textBusy) return;
    const base = filePath.split(/[\\/]/).pop() || "document.pdf";
    const fileName = `${base.replace(/\.pdf$/i, "")}.txt`;

    textBusy = true;
    error = null;
    try {
      const result = await backend.exportText({ handle: doc.handle, fileName, vault: null });
      if (result.path === null && result.characters === 0) return; // cancelled
      if (result.characters === 0) {
        await showAlert(
          "There was no text to extract. This looks like a scan — run OCR on it first, then export again.",
          "Nothing to export",
        );
        return;
      }
      showToast(result.path ? `Saved ${result.path}.` : `Downloaded ${fileName}.`, {
        title: "Plain text",
      });
    } catch (e) {
      error = formatError(e);
    } finally {
      textBusy = false;
    }
  }

  /** The tools whose behaviour is not obvious from their name. */
  function toolHint(id: Tool): string | null {
    if (id === "select") return "Select — drag over text to select it, click a mark to delete it";
    // Erase and Redact both "remove" something, and picking the wrong
    // one is not symmetrical: erasing when you meant to redact leaves
    // the text on the page, and both hints say which side of that line
    // the tool is on rather than only what it does.
    if (id === "erase") {
      return "Erase — remove markup you have added. Leaves the document's own content untouched";
    }
    if (id === "redact") {
      return "Redact — remove the document's own content in the area you drag. Gone from the file, not covered over";
    }
    if (id === "moveText" || id === "moveImage") {
      return `${id === "moveText" ? "Move text" : "Move image"} — hold Shift to constrain to one axis`;
    }
    return null;
  }

  // A selection belongs to one document, one page and one tool. Any of
  // those changing leaves quads drawn over text they no longer describe.
  $effect(() => {
    void doc;
    void activeTool;
    selection = null;
  });

  const handleWatermarkClick = () => requireSupporter("watermark");

  async function handleUnlock(): Promise<void> {
    gateState = { kind: "unlocking" };
    let result = await unlockSupporter(accessToken());
    if (!result.ok && result.kind === "unauthorized" && (await ensureUsableSession())) {
      // The token aged out between opening the gate and pressing the
      // button. The charge is idempotent, so asking again is free.
      result = await unlockSupporter(accessToken());
    }
    if (result.ok) {
      supporterUnlocked = true;
      gateState = { kind: "hidden" };
      runGatedTool();
      showToast("Watermark and OCR unlocked — they stay unlocked on this account.", {
        title: "Supporter",
      });
      return;
    }
    if (result.kind === "insufficient") {
      gateState = { kind: "insufficient", have: result.have, need: result.need };
      return;
    }
    if (result.kind === "unauthorized") {
      // Not "try again": trying again does nothing for a session that
      // has actually expired, and this is the one failure with a real
      // next step.
      gateState = { kind: "signed-out" };
      return;
    }
    gateState = { kind: "error", message: "Couldn't unlock — please try again." };
  }

  /** From the gate's "Sign in" / "Buy credits": hand over to the account
   * panel. The gate closes rather than stacking behind it — clicking
   * Watermark again re-checks, and that is what picks up a sign-in or a
   * top-up that happened in between. */
  function handleGateAccount(): void {
    gateState = { kind: "hidden" };
    showAccount = true;
  }

  let armedSignatureId = $state<string | null>(null);
  let annotations = $state<(AnnotationSummaryDto & { pageIndex: number })[]>([]);
  let annotationsLoading = $state(false);
  let pagesBusy = $state(false);
  let formFields = $state<FormFieldDto[]>([]);
  let formsBusy = $state(false);
  let ocrBusy = $state(false);
  let signatures = $state<SignatureInfoDto[]>([]);
  let compareBusy = $state(false);
  let compressBusy = $state(false);
  let undoRedoBusy = $state(false);
  let saveBusy = $state(false);
  // Single flag spanning every handler below that mutates the open
  // document (annotate/redact/delete-annotation/text-run edit+move/image
  // move/form-field create/form fill/OCR/page ops/undo/redo/signature
  // placement/opening a different document) — layered on top of each
  // handler's own local busy flag where one exists (formsBusy, pagesBusy,
  // ocrBusy, undoRedoBusy), not a replacement for them. Checked at each
  // handler's own entry (early-return, same shape undoRedoBusy already
  // used just for undo/redo) and passed down to PdfPage.svelte to disable
  // its gesture overlay for as long as it's set. This is a UI-level
  // mitigation for the store-level write races documented in
  // openpdfedit-session's lib.rs (see FsWorkingStore::write's
  // "Concurrent writers to the same key" doc and undo_impl's "Residual"
  // section): it makes the racing *pairs* those docs describe unreachable
  // from normal use of this shared UI (desktop and extension alike, since
  // it's the same component), it does not close the underlying residual
  // at the store/session level — see those doc comments for what's still
  // open below the UI.
  let mutationBusy = $state(false);

  // Edits go to a scratch copy, never the user's file (see OpenDoc in
  // lib.rs) — so the app has to offer an explicit save, and must not let
  // the window close with unsaved work still only in that scratch copy.
  // Entry-gated on `mutationBusy` (Phase 4 closing re-review finding):
  // every mutating handler below reassigns `doc` to a *new* handle and
  // retires the old one server-side (see the header comment), so a save
  // that resolves after a mutation already in flight would overwrite
  // `doc` with the stale pre-mutation handle it just captured into
  // `handle` above — a handle the backend has already dropped from
  // `docs`, cascading into UnknownHandle on the next call. Worse, if the
  // in-flight mutation is the one that actually produced unsaved
  // changes, this save's `doc = await backend.saveDocument(handle)`
  // would stomp `is_dirty` back to clean for an edit that never made it
  // into the saved file. ⌘S rides this same function (see
  // `handleKeydown` below), so gating here also covers the keyboard path.
  async function handleSave(): Promise<boolean> {
    if (!doc || mutationBusy) return false;
    const handle = doc.handle;
    error = null;
    saveBusy = true;
    try {
      doc = await backend.saveDocument(handle);
      return true;
    } catch (e) {
      error = formatError(e);
      return false;
    } finally {
      saveBusy = false;
    }
  }

  // Uses the finer-grained `pickSavePath` + `saveDocumentAtPath` pair
  // rather than `backend.saveDocumentAs()`'s single-call convenience —
  // same reason as `pickAndOpen` below: the picker has to run *before*
  // `error`/`saveBusy` bookkeeping, so canceling the dialog leaves any
  // already-visible error banner untouched, exactly matching this
  // function's pre-refactor behavior (`error = null` only ever ran after
  // a target path was chosen).
  // Same `mutationBusy` gate as `handleSave` above, same reason: a
  // stale pre-mutation handle stomping `doc`/`is_dirty` once the picker
  // returns. ⌘⇧S rides this function too (see `handleKeydown` below).
  async function handleSaveAs(): Promise<boolean> {
    if (!doc || mutationBusy) return false;
    const handle = doc.handle;
    const target = await backend.pickSavePath(doc.file_path);
    if (!target) return false;
    error = null;
    saveBusy = true;
    try {
      doc = await backend.saveDocumentAtPath(handle, target);
      filePath = target;
      return true;
    } catch (e) {
      error = formatError(e);
      return false;
    } finally {
      saveBusy = false;
    }
  }

  // Shared "you have unsaved changes" two-step confirm flow: Save (report
  // whether the save itself actually succeeded) / Discard / Cancel.
  // Originally inline in the onCloseRequested handler below; extracted
  // (Phase 2 final-review I3) so pickAndOpen's open-over-current-document
  // guard can run the exact same logic — same two dialogs, same three
  // outcomes — rather than duplicating it or (the pre-fix bug) skipping
  // the guard entirely and silently discarding the current document's
  // edits. `action` only changes the wording; the control flow (and, in
  // particular, "a chosen Save that actually fails must abort, not
  // proceed") is identical for every caller. Returns `true` if the caller
  // should go ahead with whatever it was about to do (close the window /
  // replace the open document), `false` if the user backed out.
  async function confirmProceedDespiteUnsavedChanges(action: "close" | "open a different document"): Promise<boolean> {
    // Every open tab counts, not just the visible one. Closing the
    // window with two dirty tabs used to prompt about one of them and
    // discard the other in silence.
    const dirty = tabs.filter((tab) => tab.doc.is_dirty);
    if (dirty.length === 0) return true;

    // More than one, and "Save and continue" can't mean a single save —
    // say which files are at stake and let the user go back and deal
    // with them, rather than saving some and losing the rest.
    if (dirty.length > 1) {
      const names = dirty.map((tab) => tab.filePath.split(/[/\\]/).pop()).join(", ");
      return await showConfirm(
        `${dirty.length} documents have unsaved changes: ${names}.\n\n` +
          `Close anyway and lose them?`,
        { title: "Unsaved changes", confirmLabel: "Discard all and close", destructive: true },
      );
    }

    // Exactly one dirty document: make sure it's the one on screen
    // before offering to save it, since `handleSave` acts on the active
    // tab.
    const index = tabs.indexOf(dirty[0]);
    if (index !== activeTabIndex) await switchToTab(index);

    const keep = await showConfirm(`You have unsaved changes. Save them before you ${action}?`, {
      title: "Unsaved changes",
      confirmLabel: "Save and continue",
      destructive: false,
    });
    if (keep) {
      // Saving failed — stay put (open window / current document) so
      // nothing is lost, exactly like the pre-extraction close-requested
      // behavior this mirrors.
      return await handleSave();
    }
    return await showConfirm("Discard your changes? They will be lost.", {
      title: "Discard changes?",
      confirmLabel: "Discard",
      destructive: true,
    });
  }

  // The window manager asks us before closing; decide here, then tell
  // the backend to finish the close.
  $effect(() => {
    const stop = backend.onCloseRequested(async () => {
      if (!(await confirmProceedDespiteUnsavedChanges("close"))) return;
      await backend.confirmClose();
    });
    return () => {
      stop.then((un) => un());
    };
  });

  // Tauri command errors arrive as the serialized `CommandError` enum,
  // i.e. an object like `{ "Doc": "page index 3 out of range" }` — so
  // `String(e)` on it yields the useless "[object Object]" the error
  // banner was showing. Pull the actual message out, whatever shape it
  // arrives in.
  function formatError(e: unknown): string {
    if (typeof e === "string") return e;
    if (e instanceof Error) return e.message;
    if (e && typeof e === "object") {
      const values = Object.values(e as Record<string, unknown>);
      const message = values.find((v) => typeof v === "string");
      if (typeof message === "string") return message;
      try {
        return JSON.stringify(e);
      } catch {
        return "Unknown error";
      }
    }
    return String(e);
  }

  // refreshFormFields/refreshSignatures' shared failure logger. Forms and
  // signatures are both live on the wasm/extension backend now (see
  // wasm.ts's `listFormFields`/`listSignatures`), so neither call site
  // actually hits this anymore in practice — but the demotion stays as a
  // defensive catch-all for a "not yet ported to the extension: ..."
  // failure, in case a future refactor routes some still-unported call
  // through one of these refresh functions. Nothing in `wasm.ts` throws
  // that message today: merge/compare landed in Phase 5 Task 2 (both real
  // now, same as forms/signatures), and the one remaining gap, OCR, is
  // deliberately *not* "not yet ported" — `notAvailableInExtension` throws
  // a distinct "not available in the extension: ..." message instead, on
  // purpose (see that helper's doc in wasm.ts), so it does NOT get
  // demoted here — an OCR failure should still log at console.error like
  // any other real failure. Any future call that genuinely is just
  // unported, and throws the "not yet ported" message, is expected and
  // demoted to console.debug rather than console.error, so it doesn't add
  // noise for an already-known gap. Anything else (a real desktop-backend
  // failure, say) still logs at console.error so it isn't silently
  // swallowed.
  function logRefreshFailure(context: string, e: unknown) {
    if (formatError(e).includes("not yet ported")) {
      console.debug(context, e);
    } else {
      console.error(context, e);
    }
  }

  // Uses the finer-grained `pickOpenPath` + `openDocument` pair rather
  // than `backend.pickAndOpenDocument()`'s single-call convenience: this
  // preserves the pre-refactor behavior of showing the attempted path in
  // the path bar (via `filePath`) even when `openDocument` itself fails,
  // which the combo method — returning `null` on either cancel or
  // failure — can't distinguish from the caller's side.
  //
  // Close-previous-document lifecycle (Phase 2 Task 2, Phase 1 final-review
  // finding 7): this is the *only* place `doc` ever gets reassigned to a
  // document opened from scratch (every edit/undo/redo call site
  // reassigns `doc` to a fresh handle for the *same* logical document —
  // see this file's header comment), so it's the one place a "previous
  // document" ever needs closing. Placed here (UI layer) rather than
  // inside `tauri.ts`/`wasm.ts`'s own `openDocument` deliberately: `doc`
  // is the only thing that reliably tracks the *current* handle for an
  // already-open document across the desktop backend's own handle
  // rotation (every mutating command — annotations, page edits, undo/redo
  // — reopens the file under a brand-new `DocHandle`, per
  // `openpdfedit-session::reopen_after_write`); a backend module has no
  // subscription to those handle changes and would only ever see the
  // handle from its own last `openDocument` call, which goes stale the
  // moment the user makes a single edit. Reading `doc?.handle` right
  // before this function reassigns `doc` is what keeps this correct.
  //
  // Unsaved-changes guard (Phase 2 final-review I3): opening a new
  // document over a dirty one used to close the previous document
  // unconditionally — silently destroying its unsaved edits, with no
  // prompt at all (the window-close path already had one; this one
  // didn't). Runs `confirmProceedDespiteUnsavedChanges` *before* the file
  // picker even opens, not after — so "Cancel" here aborts the whole open
  // attempt cleanly, without ever touching `doc`/`filePath`/the picker, the
  // same way a failed/canceled `openDocument` call already leaves the
  // still-current previous document untouched (see the catch block below).
  async function pickAndOpen() {
    if (mutationBusy) return;
    error = null;
    // No unsaved-changes prompt here any more: opening a document adds a
    // tab rather than replacing the current one, so nothing is at risk.
    const selected = await backend.pickOpenPath();
    if (!selected) return;

    await openInNewTab(selected);
  }

  /** How many times to re-ask before giving up, so a wrong password
   * doesn't turn into an unclosable loop. */
  const PASSWORD_ATTEMPTS = 3;

  /** Opens `path`, asking for a password if the document turns out to be
   * protected. Returns `null` if the user cancelled the prompt — which
   * is not an error and shouldn't leave a banner behind. */
  async function openPossiblyProtected(path: string): Promise<OpenedDocument | null> {
    try {
      return await backend.openDocument(path);
    } catch (e) {
      if (!isPasswordRequired(e)) throw e;
    }

    const name = path.split(/[/\\]/).pop();
    for (let attempt = 0; attempt < PASSWORD_ATTEMPTS; attempt++) {
      const password = await showPrompt(
        attempt === 0
          ? `"${name}" is password-protected. Enter its password to open it:`
          : `That password didn't open "${name}". Try again:`,
        { title: "Password required", confirmLabel: "Open", password: true },
      );
      if (password === null) return null;
      try {
        return await backend.openDocument(path, password);
      } catch (e) {
        // Anything that isn't a rejected password is a real failure and
        // shouldn't be retried as though the user mistyped.
        const text = String(e).toLowerCase();
        if (!text.includes("password")) throw e;
      }
    }
    throw new Error(`Couldn't open "${name}" — the password wasn't accepted.`);
  }

  /** Opens `path` in a new tab and makes it active. Documents already
   * open are focused rather than opened twice — two tabs over one
   * backend document would each hold a handle that the other's edits
   * rotate out from under. */
  async function openInNewTab(path: string) {
    const existing = tabs.findIndex((tab) => tab.filePath === path);
    if (existing !== -1) {
      await switchToTab(existing);
      return;
    }

    mutationBusy = true;
    try {
      const opened = await openPossiblyProtected(path);
      if (!opened) return;
      await adoptAsNewTab(opened, path);
    } catch (e) {
      error = formatError(e);
    } finally {
      mutationBusy = false;
    }
  }

  /** The bookkeeping a newly opened document needs, whichever route it
   * arrived by — the picker, or a row in the recent list. Extracted
   * when the second route appeared, rather than copied: a tab that got
   * only half of this is a document open with somebody else's outline
   * and form fields still on screen. */
  async function adoptAsNewTab(opened: OpenedDocument, path: string) {
    tabs.push({ doc: opened, filePath: path, zoom: 1, currentPage: 0 });
    activeTabIndex = tabs.length - 1;
    doc = opened;
    filePath = path;
    zoom = 1;
    currentPage = 0;
    activeTool = "select";
    closeSearch();
    await Promise.all([
      refreshAnnotations(),
      refreshFormFields(),
      refreshSignatures(),
      refreshOutline(),
    ]);
  }

  async function refreshSignatures() {
    // Re-fetched after open and after a form fill (see handleFillForm) —
    // NOT after every other mutating command. Corrected (fix-wave
    // re-review's I3): this used to claim signatures are "preserved
    // untouched by every mutating command here," on the theory that every
    // edit here goes through openpdfedit-doc's incremental (append-only)
    // save, which is true for annotations/pages/redact/textedit/form-field
    // creation, but false for filling a form — `fill_form_fields_cmd`
    // writes through PDFium's own form model (`Engine::fill_form_fields` +
    // `Engine::save_to_bytes`, a full rewrite of the entire file), not the
    // lopdf incremental path, so it invalidates any existing signature's
    // `/ByteRange` the same way any other full-rewrite operation would —
    // see `openpdfedit-session::forms::fill_form_fields_impl`'s doc.
    // Every *other* handler here still doesn't call this: they really do
    // go through the incremental-save path, so the list from open/last-
    // fill time stays accurate for them.
    if (!doc) return;
    try {
      signatures = await backend.listSignatures(doc.handle);
    } catch (e) {
      logRefreshFailure("failed to load signatures", e);
    }
  }

  async function refreshFormFields() {
    if (!doc) return;
    try {
      formFields = await backend.listFormFields(doc.handle);
    } catch (e) {
      logRefreshFailure("failed to load form fields", e);
    }
  }

  async function refreshAnnotations() {
    if (!doc) return;
    annotationsLoading = true;
    try {
      // One IPC round trip per page — fine for the document sizes this
      // milestone targets. A document with thousands of pages would want
      // a single "list every annotation" command instead; tracked as a
      // follow-up once that becomes a real workload, not a hypothetical.
      const perPage = await Promise.all(
        Array.from({ length: doc.page_count }, (_, pageIndex) =>
          backend.listPageAnnotations(doc!.handle, pageIndex).then((list) => list.map((a) => ({ ...a, pageIndex }))),
        ),
      );
      annotations = perPage.flat();
    } catch (e) {
      console.error("failed to load annotations", e);
    } finally {
      annotationsLoading = false;
    }
  }

  async function handleCreateAnnotation(pageIndex: number, payload: AnnotationPayload) {
    if (!doc || mutationBusy) return;
    error = null;
    mutationBusy = true;
    try {
      // Highlight/underline/strikeout: the drag gesture in PdfPage.svelte
      // only ever produces a freehand rectangle (it has no idea where the
      // real text is) — snap that to the actual characters PDFium finds
      // under it before creating the annotation, so it marks the real
      // words dragged over instead of an arbitrary box the user has to
      // eyeball onto the text themselves.
      let finalPayload = payload;
      if (payload.annotation.kind === "highlight" || payload.annotation.kind === "underline" || payload.annotation.kind === "strikeOut") {
        const [x0, y0, x1, y1] = payload.rect;
        let quads: [number, number, number, number][];
        try {
          quads = await backend.textSelectionQuads({ handle: doc.handle, pageIndex, x0, y0, x1, y1 });
        } catch (e) {
          error = formatError(e);
          return;
        }
        if (quads.length === 0) {
          await showAlert("No text found in that area — try dragging directly over the words you want to mark.");
          return;
        }
        const rectFromQuads: [number, number, number, number] = [
          Math.min(...quads.map((q) => q[0])),
          Math.min(...quads.map((q) => q[1])),
          Math.max(...quads.map((q) => q[2])),
          Math.max(...quads.map((q) => q[3])),
        ];
        finalPayload = { ...payload, rect: rectFromQuads, annotation: { ...payload.annotation, quads } };
      }

      doc = await backend.addAnnotation({
        handle: doc.handle,
        pageIndex,
        rect: finalPayload.rect,
        color: finalPayload.color,
        opacity: finalPayload.opacity,
        contents: finalPayload.contents ?? null,
        annotation: finalPayload.annotation,
      });
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
    } catch (e) {
      error = formatError(e);
    } finally {
      mutationBusy = false;
    }
  }

  async function handleRedact(pageIndex: number, rect: [number, number, number, number]) {
    if (!doc || mutationBusy) return;
    if (!(await showConfirm("The underlying text and images in this area are permanently removed, not just covered over.", { title: "Redact this area?", confirmLabel: "Redact", destructive: true }))) {
      // The drag gesture that produced `rect` is already over by the
      // time onRedact fires (PdfPage.svelte only calls it from
      // onPointerUp), so declining here just means "don't send the
      // command" — there's no in-progress drag state to roll back.
      return;
    }
    error = null;
    mutationBusy = true;
    try {
      doc = await backend.redactPage({ handle: doc.handle, pageIndex, rect });
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
    } catch (e) {
      error = formatError(e);
    } finally {
      mutationBusy = false;
    }
  }

  async function handleApplyWatermark(choices: WatermarkChoices) {
    if (!doc || mutationBusy) return;
    error = null;
    mutationBusy = true;
    try {
      doc = await backend.applyWatermark({ handle: doc.handle, ...choices });
      showWatermark = false;
      // Same post-mutation refresh pair as redact: the handle rotated, so
      // anything cached against the old one re-fetches.
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
    } catch (e) {
      error = formatError(e);
    } finally {
      mutationBusy = false;
    }
  }

  function pointInRect(x: number, y: number, rect: [number, number, number, number], pad = 2): boolean {
    return x >= rect[0] - pad && x <= rect[2] + pad && y >= rect[1] - pad && y <= rect[3] + pad;
  }

  // "editText" is click-based (see PdfPage.svelte's onPointerDown):
  // resolve *what* was clicked by re-listing this page's runs fresh (no
  // stale client-side cache to get out of sync with the document) and
  // finding the one whose bounding box contains the click point. The
  // move tools use a drag gesture instead — see handleMoveObject.
  /** What the Select tool has selected, if anything.
   *
   * One page at a time: a selection that spans pages would need the
   * character ranges of both ends and everything between, and every
   * reason to want one — copy a quote, check a number — is satisfied
   * within a page. */
  let selection = $state<{
    pageIndex: number;
    quads: [number, number, number, number][];
    text: string;
  } | null>(null);

  async function handleSelectText(
    pageIndex: number,
    rect: [number, number, number, number],
  ): Promise<void> {
    if (!doc) return;
    const [x0, y0, x1, y1] = rect;
    try {
      // The drag's own corners, not the rectangle's: dragging up the
      // page or right to left still reads in document order, and the
      // backend decides which end came first.
      const found = await backend.selectText({ handle: doc.handle, pageIndex, x0, y0: y1, x1, y1: y0 });
      selection = found.quads.length > 0 ? { pageIndex, ...found } : null;
    } catch (e) {
      error = formatError(e);
    }
  }

  /** Put the selection on the clipboard. Reported rather than silent:
   * copying is invisible by nature, and the only way to find out it did
   * not happen is to paste somewhere and find the old thing. */
  async function copySelection(): Promise<void> {
    const text = selection?.text;
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      showToast(`Copied ${text.length} character${text.length === 1 ? "" : "s"}.`);
    } catch {
      error = "Couldn't copy — this browser refused clipboard access.";
    }
  }

  async function handleToolClick(pageIndex: number, x: number, y: number) {
    if (!doc || mutationBusy) return;
    error = null;

    if (activeTool === "select" || activeTool === "erase") {
      // Click-to-delete: the "select" tool has no drag gesture of its
      // own (see PdfPage.svelte's onPointerDown), so a plain click here
      // hit-tests the already-loaded `annotations` list (kept fresh by
      // refreshAnnotations, called after every edit) rather than a fresh
      // round trip — this is the same list the comments panel already
      // renders from. Topmost-last-created annotation wins on overlap,
      // same as z-order for everything else in this app.
      const hit = [...annotations].reverse().find((a) => a.pageIndex === pageIndex && pointInRect(x, y, a.rect));
      if (!hit) return;
      // The dedicated Erase tool deletes on click without prompting —
      // confirming every stroke would make erasing several marks
      // painful, and ⌘Z undoes any mistake. The Select tool still
      // confirms, since a click there is likelier to be accidental.
      if (activeTool === "select") {
        const ok = await showConfirm(
          `Delete this ${hit.subtype.toLowerCase()} annotation? You can undo this with ⌘Z.`,
          { title: "Delete annotation?", confirmLabel: "Delete", destructive: true },
        );
        if (!ok) return;
      }
      mutationBusy = true;
      try {
        doc = await backend.deleteAnnotation({ handle: doc.handle, pageIndex, annotationId: hit.id });
        await Promise.all([refreshAnnotations(), refreshFormFields()]);
      } catch (e) {
        error = formatError(e);
      } finally {
        mutationBusy = false;
      }
      return;
    }

    if (activeTool === "editText") {
      mutationBusy = true;
      try {
        const runs = await backend.listTextRuns(doc.handle, pageIndex);
        // Smallest containing box wins — a click inside a long line and
        // a short one should offer the short one, which is the tighter
        // match to what's under the cursor.
        const run = runs.filter((r) => pointInRect(x, y, r.rect)).sort((a, b) => area(a.rect) - area(b.rect))[0];
        if (!run) {
          await showAlert("No text found at that spot. Try clicking directly on the characters you want to change.");
          return;
        }
        if (!run.isEditable) {
          // Subset fonts in general ARE editable now (decoded via their
          // /ToUnicode table) — this only fires for the rarer case of a
          // font that ships no such table at all, typically an icon or
          // symbol font whose glyphs aren't characters in the first
          // place. Say that specifically rather than blaming subsetting.
          await showAlert(
            "This particular text can't be edited.\n\n" +
              "Its font provides no character mapping (/ToUnicode), which usually means it's an " +
              "icon or symbol font rather than real text.\n\n" +
              "Highlight, underline, strikeout and redaction still work on it.",
          );
          return;
        }
        const newText = await showPrompt("Replace this text with:", { title: "Edit text", defaultValue: run.text, confirmLabel: "Replace" });
        if (newText === null || newText.trim().length === 0) return;
        doc = await backend.editTextRun({ handle: doc.handle, pageIndex, runIndex: run.index, newText: newText.trim() });
        await Promise.all([refreshAnnotations(), refreshFormFields()]);
      } catch (e) {
        const message = formatError(e);
        // The one genuinely-unsupported case left: a character with no
        // glyph in this document's embedded font subset. Explain it in
        // terms the user can act on instead of showing a raw error.
        if (message.includes("no glyph for")) {
          await showAlert(
            message.replace(/^.*no glyph for:?/, "Can't use these characters:").trim() +
              "\n\nThis PDF only embeds the characters its text actually used, so letters that " +
              "appear nowhere in the document can't be typed into it. Try different wording.",
          );
        } else {
          error = message;
        }
      } finally {
        mutationBusy = false;
      }
      return;
    }

  }

  // Drag-to-move for the "moveText"/"moveImage" tools. `x`/`y` are where
  // the drag began — that's what identifies which run or image is being
  // moved — and `dx`/`dy` are the travel. Resolving the target from a
  // freshly re-listed page (rather than a cached client-side list) is the
  // same approach handleToolClick uses, for the same reason: the
  // document's content stream changes under us on every edit.
  async function handleMoveObject(pageIndex: number, x: number, y: number, dx: number, dy: number) {
    if (!doc || mutationBusy) return;
    error = null;
    mutationBusy = true;
    try {
      if (activeTool === "moveText") {
        const runs = await backend.listTextRuns(doc.handle, pageIndex);
        // Smallest containing box wins: lines overlap, and the tighter
        // match is nearly always the one under the cursor.
        const hits = runs.filter((r) => pointInRect(x, y, r.rect));
        const run = hits.sort((a, b) => area(a.rect) - area(b.rect))[0];
        if (!run) {
          await showAlert("No text found where you started dragging. Press directly on the text you want to move, then drag.");
          return;
        }
        doc = await backend.moveTextRun({ handle: doc.handle, pageIndex, runIndex: run.index, dx, dy });
      } else {
        const placements = await backend.listImagePlacements(doc.handle, pageIndex);
        const hits = placements.filter((p) => pointInRect(x, y, p.rect));
        const placement = hits.sort((a, b) => area(a.rect) - area(b.rect))[0];
        if (!placement) {
          await showAlert("No image found where you started dragging. Press directly on the image you want to move, then drag.");
          return;
        }
        doc = await backend.moveImage({ handle: doc.handle, pageIndex, placementIndex: placement.index, dx, dy });
      }
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
    } catch (e) {
      error = formatError(e);
    } finally {
      mutationBusy = false;
    }
  }

  function area(rect: [number, number, number, number]): number {
    return Math.abs(rect[2] - rect[0]) * Math.abs(rect[3] - rect[1]);
  }

  /** A field name that's unique in this document, so the naming prompt
   * can be answered by just pressing Enter. A form field must have a
   * name — it's the key its value is stored under — but making someone
   * invent one before they can draw a box is a poor trade. */
  function suggestFieldName(kind: "text" | "checkbox"): string {
    const base = kind === "text" ? "text" : "checkbox";
    const taken = new Set(formFields.map((f) => f.name));
    for (let n = 1; ; n++) {
      const candidate = `${base}_${n}`;
      if (!taken.has(candidate)) return candidate;
    }
  }

  async function handleCreateField(pageIndex: number, rect: [number, number, number, number], kind: "text" | "checkbox") {
    if (!doc || mutationBusy) return;
    // No naming prompt. A field's name is the key its value is stored
    // under — it matters when a form's data gets exported, and not at
    // all to someone adding a box to type in — so it's generated here
    // and the dialog that used to ask for it is gone. Drawing a box and
    // then being interrogated about it is the wrong order: the drawing
    // *was* the instruction.
    const name = suggestFieldName(kind);
    error = null;
    mutationBusy = true;
    try {
      doc = await backend.createFormField({ handle: doc.handle, pageIndex, rect, kind, name });
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
      if (kind === "text") {
        // The field is only clickable under the select tool (a markup
        // gesture has to pass through fields, or drawing over a form
        // would be swallowed by them). Staying on "Add text field" is
        // what made a just-drawn box look inert, so switch, then put the
        // cursor in it: draw, type, done.
        activeTool = "select";
        focusField = { name, nonce: focusField ? focusField.nonce + 1 : 1 };
      } else {
        // A checkbox still needs the panel — an on-page control for it
        // would be a toggle, not a text box, and isn't built yet.
        showForms = true;
        showToast(`Added "${name}". Tick it in the Form fields panel.`, { title: "Checkbox" });
      }
    } catch (e) {
      error = formatError(e);
    } finally {
      mutationBusy = false;
    }
  }

  // Places the currently-armed saved signature into `rect` (the
  // drag-rectangle from PdfPage.svelte's "signature" tool gesture),
  // preserving the signature's own aspect ratio and centering it within
  // the drag rather than stretching it to fill an arbitrary box — a
  // signature stretched off-proportion doesn't look like a signature
  // anymore. Placed as an Ink annotation through the exact same
  // `add_annotation_cmd` path the Draw tool already uses (see
  // signatures.svelte.ts's module doc for why this needed no new
  // backend surface at all): a saved signature is just a reusable
  // template for that same ink geometry.
  function handlePlaceSignature(pageIndex: number, rect: [number, number, number, number]) {
    if (!doc || mutationBusy) return;
    if (!armedSignatureId) {
      showAlert("Pick a saved signature in the Signatures panel first (or draw a new one), then drag on the page to place it.", "No signature selected");
      return;
    }
    const sig = savedSignatures().find((s) => s.id === armedSignatureId);
    if (!sig) {
      armedSignatureId = null;
      showAlert("That saved signature is no longer available.");
      return;
    }

    const [x0, y0, x1, y1] = rect;
    const rectW = x1 - x0;
    const rectH = y1 - y0;
    const targetAspect = rectW / rectH;
    // "Contain" fit: shrink to whichever axis the drag rect constrains
    // more tightly, matching the signature's own proportions.
    const fitW = sig.aspect > targetAspect ? rectW : rectH * sig.aspect;
    const fitH = sig.aspect > targetAspect ? rectW / sig.aspect : rectH;
    const offsetX = x0 + (rectW - fitW) / 2;
    const offsetY = y0 + (rectH - fitH) / 2;

    // Saved strokes are normalized 0..1 in the pad's canvas space
    // (y-down); PDF page space is y-up, so a drawing's top (ny=0) has
    // to land at the *higher* PDF y — the top of the fitted box.
    const mapped: [number, number][][] = sig.strokes.map((stroke) =>
      stroke.map(([nx, ny]) => [offsetX + nx * fitW, offsetY + (1 - ny) * fitH] as [number, number]),
    );
    const points = mapped.flat();
    const xs = points.map((p) => p[0]);
    const ys = points.map((p) => p[1]);
    const finalRect: [number, number, number, number] = [Math.min(...xs) - 2, Math.min(...ys) - 2, Math.max(...xs) + 2, Math.max(...ys) + 2];

    handleCreateAnnotation(pageIndex, {
      rect: finalRect,
      // Always solid black, independent of the highlight/underline
      // colour swatches — a signature isn't markup, and Adobe's own
      // default signature ink is black regardless of whatever colour a
      // user last picked for highlighting.
      color: [0, 0, 0],
      opacity: 1,
      annotation: { kind: "ink", strokes: mapped },
    });
  }

  function handleSignatureSaved(name: string, strokes: [number, number][][], aspect: number) {
    const sig = addSignature(name, strokes, aspect);
    armedSignatureId = sig.id;
    showSignaturePad = false;
  }

  // Shared tail for every page-organization action: page ops (like
  // annotation writes) return a *new* handle because the underlying file
  // changed — see pages.rs's module doc — so `doc` is reassigned wholesale,
  // and annotations/form fields are re-fetched since they're keyed to the
  // handle that just went stale, even when the edit itself (a page
  // reorder, say) didn't touch either.
  async function runPagesOp(fn: () => Promise<OpenedDocument>) {
    if (mutationBusy) return;
    error = null;
    pagesBusy = true;
    mutationBusy = true;
    try {
      doc = await fn();
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
    } catch (e) {
      error = formatError(e);
    } finally {
      pagesBusy = false;
      mutationBusy = false;
    }
  }

  function handleRotate(pageIndex: number, deltaDegrees: number) {
    if (!doc) return;
    const handle = doc.handle;
    runPagesOp(() => backend.rotatePage(handle, pageIndex, deltaDegrees));
  }

  function handleDelete(pageIndex: number) {
    if (!doc) return;
    const handle = doc.handle;
    runPagesOp(() => backend.deletePage(handle, pageIndex));
  }

  function handleMove(pageIndex: number, direction: "Up" | "Down") {
    if (!doc) return;
    const handle = doc.handle;
    runPagesOp(() => backend.movePage(handle, pageIndex, direction));
  }

  function handleCrop(pageIndex: number, rect: [number, number, number, number]) {
    if (!doc) return;
    const handle = doc.handle;
    runPagesOp(() => backend.setCropBox(handle, pageIndex, rect));
  }

  async function handleMerge() {
    if (!doc) return;

    // The open document's *edited* state only exists in its working
    // copy, not on disk at its original path — re-picking the same file
    // from the dialog below would silently merge in the stale, pre-edit
    // version. Ask up front instead of quietly excluding it.
    const includeOpen = await showConfirm(
      doc.is_dirty
        ? "This document has unsaved changes. Include its current content in the merge?"
        : "Include the document you currently have open in this merge?",
      { title: "Merge PDFs", confirmLabel: "Include it" },
    );

    // The two picker calls below are wrapped in their own try/catch
    // (rather than left bare, as they used to be) so a backend that
    // can't fulfil a picker call — the wasm backend's `pickOpenPaths`
    // was `notImplemented` and threw synchronously until it was ported —
    // surfaces through the same error banner (`error = formatError(e)`)
    // every other failure in this flow already uses, instead of an
    // unhandled promise rejection nothing on screen reflects. On desktop
    // neither picker call ever rejects outside of this, so this changes
    // nothing observable there.
    let sourcePaths: string[];
    let outputPath: string | null;
    try {
      sourcePaths = await backend.pickOpenPaths();
      // Empty-sources return (fix-round C1): sourcePaths is empty here by
      // definition, so there is nothing to release — releasePicks is
      // still called for symmetry with the cancel-of-save-target return
      // below, and because it's a harmless no-op on an empty array.
      if (!includeOpen && sourcePaths.length === 0) {
        await backend.releasePicks(sourcePaths);
        return;
      }
      outputPath = await backend.pickSavePath("merged.pdf");
    } catch (e) {
      error = formatError(e);
      return;
    }
    if (!outputPath) {
      // Cancel-of-save-target return (fix-round C1): sourcePaths was
      // already picked above (each stashed under a synthetic key on the
      // wasm backend — see wasm.ts's "Open-document bookkeeping" doc), but
      // canceling the save dialog means mergeDocuments never runs to
      // consume them. Without this, those picks sit in pendingOpenPicks
      // forever — permanently stealing their filenames' un-suffixed pick
      // keys from every later pick of the same name (the concrete repro:
      // open a.pdf -> Merge -> pick b.pdf as a source -> cancel the save
      // dialog -> later "open b.pdf" gets keyed "b.pdf (2)" forever, and
      // compareDocuments' pathA scan then can't find it).
      await backend.releasePicks(sourcePaths);
      return;
    }

    await runPagesOp(async () => {
      const result = await backend.mergeDocuments({ openHandle: includeOpen ? doc!.handle : null, sourcePaths, outputPath });
      filePath = outputPath;
      return result;
    });
  }

  async function handleExtractSelected(pageIndices: number[]) {
    if (!doc || pageIndices.length === 0) return;
    const handle = doc.handle;
    const outputPath = await backend.pickSavePath("extracted.pdf");
    if (!outputPath) return;

    await runPagesOp(async () => {
      const result = await backend.extractPages({ handle, pageIndices, outputPath });
      filePath = outputPath;
      return result;
    });
  }

  /** Commits one field, edited in place on the page. Reuses the same
   * fill path the panel uses, so there's one way a value reaches the
   * document rather than two that can diverge. */
  async function handleFillFieldInline(name: string, value: string) {
    const current = formFields.find((f) => f.name === name)?.value ?? "";
    // Blur fires on every field a user tabs through; only a real change
    // is worth a document mutation and an undo entry.
    if (value === current) return;
    await handleFillForm({ [name]: value });
  }

  async function handleFillForm(values: Record<string, string>) {
    if (!doc || mutationBusy) return;
    const handle = doc.handle;
    error = null;
    formsBusy = true;
    mutationBusy = true;
    try {
      doc = await backend.fillFormFields({ handle, values });
      // Also refreshes signatures (unlike every other handler here) — a
      // fill writes through PDFium's own full-rewrite save path, which
      // invalidates any existing signature's byte range; see
      // refreshSignatures' own comment for why this is the one mutating
      // command that needs it.
      await Promise.all([refreshAnnotations(), refreshFormFields(), refreshSignatures()]);
    } catch (e) {
      error = formatError(e);
    } finally {
      formsBusy = false;
      mutationBusy = false;
    }
  }

  /** The OCR button's click — through the Supporter gate, same as the
   * watermark. `runOcr` below is what actually runs once it's allowed. */
  const handleOcrDocument = () => requireSupporter("ocr");

  /** What OCR can be asked to read.
   *
   * Tesseract recognises the script it has trained data for and nothing
   * else — handed the wrong one it returns nothing, or nonsense, without
   * failing. OCR shipped hard-wired to English, so running it on a
   * Chinese document looked exactly like a broken feature: it worked,
   * took its time, and added an empty text layer.
   *
   * The CJK entries include `eng` because those documents almost always
   * carry Latin digits, headings and units, and Tesseract will read both
   * from one page when given both. The Latin languages do not: adding
   * English to them costs accuracy by giving the recogniser two
   * plausible readings of the same letters. */
  const OCR_LANGUAGES = [
    { value: "eng", label: "English" },
    { value: "chi_sim+eng", label: "中文 (简体) — Chinese, Simplified" },
    { value: "chi_tra+eng", label: "中文 (繁體) — Chinese, Traditional" },
    { value: "jpn+eng", label: "日本語 — Japanese" },
    { value: "kor+eng", label: "한국어 — Korean" },
    { value: "fra", label: "Français — French" },
    { value: "deu", label: "Deutsch — German" },
    { value: "spa", label: "Español — Spanish" },
    { value: "por", label: "Português — Portuguese" },
    { value: "ita", label: "Italiano — Italian" },
    { value: "rus", label: "Русский — Russian" },
    { value: "ara", label: "العربية — Arabic" },
  ];

  const OCR_LANG_KEY = "openpdfedit.ocr.lang";

  /** Last language used, so someone working through a stack of documents
   * in one language answers once. Storage can throw outright in a
   * private window, so neither read nor write is allowed to take OCR
   * down with it. */
  function rememberedOcrLang(): string {
    try {
      const stored = localStorage.getItem(OCR_LANG_KEY);
      if (stored && OCR_LANGUAGES.some((l) => l.value === stored)) return stored;
    } catch {
      /* no storage: the default is fine */
    }
    return "eng";
  }

  async function runOcr() {
    if (!doc || mutationBusy) return;
    const handle = doc.handle;

    const lang = await showChoice("Which language is the text in?", {
      title: "Recognise text",
      choices: OCR_LANGUAGES,
      defaultValue: rememberedOcrLang(),
      confirmLabel: "Run OCR",
    });
    if (lang === null) return;
    try {
      localStorage.setItem(OCR_LANG_KEY, lang);
    } catch {
      /* answering again next time is a smaller problem than failing here */
    }

    error = null;
    ocrBusy = true;
    mutationBusy = true;
    try {
      doc = await backend.ocrDocument({ handle, lang });
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
    } catch (e) {
      const message = formatError(e);
      // "Tesseract not found" is a setup instruction, not an error the
      // one-line banner can usefully carry — it's multi-line and tells
      // the user exactly what to install.
      if (message.includes("Tesseract OCR was not found")) {
        await showAlert(message, "OCR needs Tesseract installed");
      } else {
        error = message;
      }
    } finally {
      ocrBusy = false;
      mutationBusy = false;
    }
  }

  function formatByteSize(n: number): string {
    if (n >= 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    if (n >= 1024) return `${Math.round(n / 1024)} KB`;
    return `${n} B`;
  }

  // Export-shaped like handleExtractSelected: pick a target, write a
  // compressed (full-rewrite) copy, never touch the open document. The
  // signature caveat in the confirm text is real — see
  // compress_document_to_path_impl's doc.
  async function handleCompressDocument() {
    if (!doc || compressBusy) return;
    if (
      !(await showConfirm(
        "Saves a compressed copy: the document is fully rewritten, shedding edit history and unused data. Existing digital signatures will not carry over into the copy. The open document itself is unchanged.",
        { title: "Save a compressed copy?", confirmLabel: "Choose where to save" },
      ))
    ) {
      return;
    }
    const outputPath = await backend.pickSavePath("compressed.pdf");
    if (!outputPath) return;
    error = null;
    compressBusy = true;
    try {
      const stats = await backend.compressDocument({ handle: doc.handle, outputPath });
      const pct =
        stats.beforeBytes > 0 ? Math.round((1 - stats.afterBytes / stats.beforeBytes) * 100) : 0;
      showToast(
        `Compressed copy saved: ${formatByteSize(stats.beforeBytes)} → ${formatByteSize(stats.afterBytes)}${pct > 0 ? ` (${pct}% smaller)` : ""}`,
      );
    } catch (e) {
      error = formatError(e);
    } finally {
      compressBusy = false;
    }
  }

  async function handleCompareDocument() {
    if (!filePath) return;
    const otherPath = await backend.pickOpenPath();
    if (!otherPath) return;

    error = null;
    compareBusy = true;
    try {
      const report = await backend.compareDocuments({ pathA: filePath, pathB: otherPath, pixelTargetWidth: 800 });
      await showCompareReport(report, otherPath);
    } catch (e) {
      error = formatError(e);
    } finally {
      compareBusy = false;
    }
  }

  async function showCompareReport(report: CompareReportDto, otherPath: string) {
    const lines = [`Comparing the open document against:\n  ${otherPath}`, `Pages: ${report.pageCountA} vs ${report.pageCountB}`];

    if (report.textPages.length === 0) {
      lines.push("Text: no differences found (see openpdfedit-compare's docs for this mode's limitations — it diffs text runs, not words).");
    } else {
      lines.push(`Text: ${report.textPages.length} page(s) with differing text runs:`);
      for (const p of report.textPages.slice(0, 10)) {
        lines.push(`  Page ${p.pageIndex + 1}: -${p.removed.length} run(s), +${p.added.length} run(s)`);
      }
      if (report.textPages.length > 10) lines.push(`  ...and ${report.textPages.length - 10} more page(s)`);
    }

    if (report.pixelPages.length > 0) {
      const changed = report.pixelPages.filter((p) => p.differingPixels > 0);
      lines.push(
        changed.length === 0
          ? "Pixels: no rendering differences found."
          : `Pixels: ${changed.length} page(s) render differently (of ${report.pixelPages.length} compared).`,
      );
    }

    await showAlert(lines.join("\n"), "Compare result");
  }

  async function handleUndo() {
    if (!doc || !doc.can_undo || undoRedoBusy || mutationBusy) return;
    const handle = doc.handle;
    error = null;
    undoRedoBusy = true;
    mutationBusy = true;
    try {
      doc = await backend.undo(handle);
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
    } catch (e) {
      error = formatError(e);
    } finally {
      undoRedoBusy = false;
      mutationBusy = false;
    }
  }

  async function handleRedo() {
    if (!doc || !doc.can_redo || undoRedoBusy || mutationBusy) return;
    const handle = doc.handle;
    error = null;
    undoRedoBusy = true;
    mutationBusy = true;
    try {
      doc = await backend.redo(handle);
      await Promise.all([refreshAnnotations(), refreshFormFields()]);
    } catch (e) {
      error = formatError(e);
    } finally {
      undoRedoBusy = false;
      mutationBusy = false;
    }
  }

  // Cmd+Z / Cmd+Shift+Z (Ctrl on non-Mac) — the standard shortcut users
  // reach for before ever looking at a toolbar button. Skipped while an
  // annotation-comment prompt or similar native dialog might be focused
  // is not something we can detect from here, but window.prompt/alert are
  // modal and block the event loop anyway, so this can't fire mid-dialog.
  function handleKeydown(e: KeyboardEvent) {
    const meta = e.metaKey || e.ctrlKey;
    if (meta && e.key.toLowerCase() === "f") {
      e.preventDefault();
      openSearch();
      return;
    }
    // ⌘G / ⌘⇧G step through matches from anywhere, so you can leave the
    // find box and keep going without clicking back into it.
    if (meta && e.key.toLowerCase() === "g") {
      e.preventDefault();
      stepSearch(e.shiftKey ? -1 : 1);
      return;
    }
    if (e.key === "Escape" && showSearch) {
      e.preventDefault();
      closeSearch();
      return;
    }
    // ⌘C only when this app owns a selection and the keystroke did not
    // come from somewhere with its own text — a form field, the find
    // box. Copying what you just typed must keep working.
    const typing =
      e.target instanceof HTMLElement &&
      (e.target.tagName === "INPUT" ||
        e.target.tagName === "TEXTAREA" ||
        e.target.isContentEditable);
    if (meta && e.key.toLowerCase() === "c" && selection && !typing) {
      e.preventDefault();
      void copySelection();
      return;
    }
    if (meta && e.key.toLowerCase() === "s") {
      e.preventDefault();
      if (e.shiftKey) handleSaveAs();
      else handleSave();
      return;
    }
    if (meta && canPrint && e.key.toLowerCase() === "p") {
      // Preventing the default matters: the browser's own ⌘P would
      // print the *editor* — toolbars, panels and all — rather than the
      // document being edited.
      e.preventDefault();
      handlePrint();
      return;
    }
    if (!meta || e.key.toLowerCase() !== "z") return;
    e.preventDefault();
    if (e.shiftKey) {
      handleRedo();
    } else {
      handleUndo();
    }
  }

  async function showSignatureDetails() {
    const lines = signatures.map((s, i) => {
      const parts = [
        `Signature ${i + 1}`,
        `  Signer: ${s.name ?? "(not stated)"}`,
        `  Reason: ${s.reason ?? "(not stated)"}`,
        `  Signed: ${s.signingTime ?? "(not stated)"}`,
        `  Format: ${s.subFilter ?? "(unknown)"}`,
        `  Byte range looks structurally sound: ${s.byteRangeIsStructurallySound ? "yes" : "no"}`,
      ];
      return parts.join("\n");
    });
    await showAlert(
      "NOT CRYPTOGRAPHICALLY VERIFIED — this only shows what the document itself claims.\n" +
        "OpenPdfEdit does not yet check the signature is genuine, untampered, or trusted.\n\n" +
        lines.join("\n\n"),
    );
  }

  function zoomIn() {
    zoom = Math.min(MAX_ZOOM, zoom * ZOOM_STEP);
  }
  function zoomOut() {
    zoom = Math.max(MIN_ZOOM, zoom / ZOOM_STEP);
  }
  function zoomReset() {
    zoom = 1;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<DialogHost />
<ToastHost />
{#if showSignaturePad}
  <SignaturePad onSave={handleSignatureSaved} onCancel={() => (showSignaturePad = false)} />
{/if}

<main class="shell">
  <header class="topbar">
    <BrandMark size={17} />

    <button class="oa-btn oa-btn--ghost" onclick={pickAndOpen}>
      <Icon name="folder-open" size={15} />
      Open PDF…
    </button>

    {#if doc}
      <div class="topbar__group">
        <button
          class="oa-icon-btn oa-icon-btn--sm"
          onclick={handleSave}
          disabled={!doc.is_dirty || saveBusy || mutationBusy}
          use:tooltip={doc.is_dirty
            ? savesByDownloading
              ? "Download the edited copy (⌘S) — this browser can't write back over the original"
              : "Save to the original file (⌘S)"
            : "Saved"}
          aria-label={savesByDownloading ? "Download a copy" : "Save"}
        >
          <Icon name="save" size={15} spin={saveBusy} />
        </button>
        <button class="oa-icon-btn oa-icon-btn--sm" onclick={handleUndo} disabled={!doc.can_undo || undoRedoBusy || mutationBusy} use:tooltip={"Undo (⌘Z)"} aria-label="Undo">
          <Icon name="undo-2" size={15} />
        </button>
        <button class="oa-icon-btn oa-icon-btn--sm" onclick={handleRedo} disabled={!doc.can_redo || undoRedoBusy || mutationBusy} use:tooltip={"Redo (⌘⇧Z)"} aria-label="Redo">
          <Icon name="redo-2" size={15} />
        </button>
      </div>

      <div class="topbar__group topbar__group--zoom">
        <ZoomControl
          {zoom}
          canZoomOut={zoom > MIN_ZOOM}
          canZoomIn={zoom < MAX_ZOOM}
          onZoomOut={zoomOut}
          onZoomIn={zoomIn}
          onReset={zoomReset}
        />
      </div>

      <span class="oa-caption topbar__meta">
        Page {currentPage + 1} of {doc.page_count}
      </span>
    {/if}

    {#if doc}
      <!-- Phone only (see the media query): the document tools do not fit a
           390px topbar, so they move to a sheet. The buttons themselves are
           not duplicated — the same groups are repositioned by CSS, which is
           what keeps one set of handlers and one set of disabled states. -->
      <button
        class="topbar__more oa-icon-btn oa-icon-btn--sm"
        class:oa-icon-btn--selected={moreOpen}
        onclick={() => (moreOpen = !moreOpen)}
        aria-label="More tools"
        aria-expanded={moreOpen}
      >
        <Icon name="layout-panel-left" size={15} />
      </button>
    {/if}
    <div class="topbar__spacer"></div>


    <button
      class="oa-icon-btn oa-icon-btn--sm account-btn"
      class:account-btn--in={signedIn}
      onclick={() => (showAccount = true)}
      use:tooltip={signedIn ? "Account — signed in" : "Account — sign in, credits"}
      aria-label={signedIn ? "Account — signed in" : "Account"}
    >
      <Icon name="circle-user" size={15} />
    </button>
  </header>

    {#if doc}
      <div class="tools topbar__group--overflow" class:is-open={moreOpen}>
        <div class="tools__section">
          <p class="tools__heading">Panels</p>
          <div class="tools__group">
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              class:oa-icon-btn--selected={showComments}
              onclick={() => (showComments = !showComments)}
              use:tooltip={"Comments"}
              aria-label="Toggle comments panel"
            >
              <Icon name="message-square" size={15} />
            <span class="tools__label">Comments</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              class:oa-icon-btn--selected={showSearch}
              onclick={() => (showSearch ? closeSearch() : openSearch())}
              use:tooltip={"Find in document (⌘F)"}
              aria-label="Find in document"
            >
              <Icon name="search" size={15} />
            <span class="tools__label">Find</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              class:oa-icon-btn--selected={showOutline}
              onclick={() => (showOutline = !showOutline)}
              use:tooltip={outline.length > 0 ? "Contents" : "Contents — this document has no bookmarks"}
              aria-label="Toggle contents panel"
            >
              <Icon name="list-tree" size={15} />
            <span class="tools__label">Contents</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              class:oa-icon-btn--selected={showPages}
              onclick={() => (showPages = !showPages)}
              use:tooltip={"Pages"}
              aria-label="Toggle pages panel"
            >
              <Icon name="layout-panel-left" size={15} />
            <span class="tools__label">Pages</span>
            </button>
            {#if formFields.length > 0}
              <button
                class="oa-icon-btn oa-icon-btn--sm"
                class:oa-icon-btn--selected={showForms}
                onclick={() => (showForms = !showForms)}
                use:tooltip={"Form fields"}
                aria-label="Toggle form fields panel"
              >
                <Icon name="list-checks" size={15} />
              <span class="tools__label">Form fields</span>
              </button>
            {/if}
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              class:oa-icon-btn--selected={showSignatures}
              onclick={() => (showSignatures = !showSignatures)}
              use:tooltip={"Saved signatures — draw one, then drag it onto the page"}
              aria-label="Toggle signatures panel"
            >
              <Icon name="signature" size={15} />
            <span class="tools__label">Signatures</span>
            </button>
          </div>
        </div>
        <div class="tools__section">
          <p class="tools__heading">Document</p>
          <div class="tools__group">
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              class:oa-icon-btn--selected={showNumbering}
              onclick={() => (showNumbering = !showNumbering)}
              use:tooltip={"Add page numbers or Bates numbering"}
              aria-label="Number pages"
            >
              <Icon name="hash" size={15} />
            <span class="tools__label">Page numbers</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              onclick={handleOcrDocument}
              disabled={ocrBusy}
              use:tooltip={backendKind === "wasm"
                ? "Make a scanned document searchable — the first run downloads the recogniser (about 3 MB), then works offline"
                : "Make a scanned document searchable (requires tesseract installed locally)"}
              aria-label="OCR document"
            >
              <Icon name="scan-text" size={15} spin={ocrBusy} />
            <span class="tools__label">OCR</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              class:oa-icon-btn--selected={showWatermark}
              onclick={handleWatermarkClick}
              disabled={mutationBusy}
              use:tooltip={"Watermark — tile text or a logo across every page"}
              aria-label="Watermark document"
            >
              <Icon name="stamp" size={15} />
            <span class="tools__label">Watermark</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              onclick={handleFlatten}
              disabled={flattenBusy || mutationBusy}
              use:tooltip={"Flatten — bake markup into the page so it can't be edited or removed"}
              aria-label="Flatten"
            >
              <Icon name="layers" size={15} spin={flattenBusy} />
            <span class="tools__label">Flatten</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              onclick={handleCompareDocument}
              disabled={compareBusy}
              use:tooltip={"Compare the open document against another PDF (text and rendered-pixel differences)"}
              aria-label="Compare documents"
            >
              <Icon name="git-compare" size={15} spin={compareBusy} />
            <span class="tools__label">Compare</span>
            </button>
          </div>
        </div>
        <div class="tools__section">
          <p class="tools__heading">Markup</p>
          <div class="tools__group">
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              onclick={handleRemoveMarkup}
              disabled={unmarkBusy || mutationBusy}
              use:tooltip={"Remove markup — delete annotations, and pen layers already flattened into the page"}
              aria-label="Remove markup"
            >
              <Icon name="eraser" size={15} spin={unmarkBusy} />
            <span class="tools__label">Remove markup</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              onclick={handleExportMarkdown}
              disabled={markdownBusy}
              use:tooltip={"Convert the document's text to Markdown — save it as a file, or into an Obsidian vault"}
              aria-label="Export as Markdown"
            >
              <Icon name="file-code" size={15} spin={markdownBusy} />
              <span class="tools__label">Markdown</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              onclick={handleExportText}
              disabled={textBusy}
              use:tooltip={"Extract the document's text to a plain .txt file — no formatting, just the words"}
              aria-label="Export as plain text"
            >
              <Icon name="file-text" size={15} spin={textBusy} />
              <span class="tools__label">Text</span>
            </button>
          </div>
        </div>
        <div class="tools__section">
          <p class="tools__heading">Save & protect</p>
          <div class="tools__group">
            <button class="oa-icon-btn oa-icon-btn--sm" onclick={handleSaveAs} disabled={saveBusy || mutationBusy} use:tooltip={"Save a copy (⌘⇧S)"} aria-label="Save as">
              <Icon name="copy" size={15} />
            <span class="tools__label">Save a copy</span>
            </button>
            {#if canPrint}
              <button class="oa-icon-btn oa-icon-btn--sm" onclick={handlePrint} disabled={printBusy || mutationBusy} use:tooltip={"Print (⌘P)"} aria-label="Print">
                <Icon name="printer" size={15} spin={printBusy} />
              <span class="tools__label">Print</span>
              </button>
            {/if}
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              onclick={handleCompressDocument}
              disabled={compressBusy}
              use:tooltip={"Save a compressed copy — full rewrite, sheds edit history and unused data"}
              aria-label="Save a compressed copy"
            >
              <Icon name="file-archive" size={15} spin={compressBusy} />
            <span class="tools__label">Compress</span>
            </button>
            <button
              class="oa-icon-btn oa-icon-btn--sm"
              class:oa-icon-btn--selected={showEncrypt}
              onclick={() => (showEncrypt = !showEncrypt)}
              use:tooltip={"Save a password-protected copy"}
              aria-label="Protect with a password"
            >
              <Icon name="key" size={15} />
            <span class="tools__label">Password</span>
            </button>
          </div>
        </div>
        {#if signatures.length > 0}
          <button class="oa-badge oa-badge--warning topbar__pill-btn" onclick={showSignatureDetails} use:tooltip={"Signature info is structural only — not cryptographically verified"}>
            <Icon name="shield-alert" size={13} />
            {signatures.length} signature{signatures.length === 1 ? "" : "s"}
          </button>
        {/if}
      </div>
    {/if}

  <AccountPanel open={showAccount} onClose={() => (showAccount = false)} />
  <EncryptPanel
    open={showEncrypt}
    busy={encryptBusy}
    onApply={handleEncrypt}
    onClose={() => (showEncrypt = false)}
  />
  <NumberingPanel
    open={showNumbering}
    busy={mutationBusy}
    pageCount={doc?.page_count ?? 0}
    onApply={handleNumberPages}
    onClose={() => (showNumbering = false)}
  />
  <WatermarkPanel open={showWatermark} busy={mutationBusy} onApply={handleApplyWatermark} onClose={() => (showWatermark = false)} />
  <SupporterGate
    state={gateState}
    tool={gatedTool}
    onAccount={handleGateAccount}
    onUnlock={handleUnlock}
    onRetry={() => requireSupporter(gatedTool)}
    onClose={() => (gateState = { kind: "hidden" })}
  />

  {#if doc && showSearch}
    <div class="find-bar">
      <Icon name="search" size={14} />
      <input
        class="find-bar__input"
        bind:this={searchInputEl}
        bind:value={searchQuery}
        oninput={scheduleSearch}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            // Enter before the debounce has fired should search now
            // rather than do nothing and feel broken.
            if (searchHits.length === 0 && !searchRan) {
              clearTimeout(searchDebounce);
              runSearch();
            } else {
              stepSearch(e.shiftKey ? -1 : 1);
            }
          }
        }}
        type="text"
        placeholder="Find in document"
        spellcheck="false"
        aria-label="Find in document"
      />

      <span class="find-bar__count oa-mono">
        {#if searchBusy && !searchRan}
          …
        {:else if searchHits.length > 0}
          {searchActiveIndex + 1} of {searchHits.length}{searchTruncated ? "+" : ""}
        {:else if searchRan}
          No matches
        {/if}
      </span>

      <button
        class="oa-icon-btn oa-icon-btn--sm"
        onclick={() => stepSearch(-1)}
        disabled={searchHits.length === 0}
        use:tooltip={"Previous match (⇧Enter)"}
        aria-label="Previous match"
      >
        <Icon name="chevron-up" size={15} />
      </button>
      <button
        class="oa-icon-btn oa-icon-btn--sm"
        onclick={() => stepSearch(1)}
        disabled={searchHits.length === 0}
        use:tooltip={"Next match (Enter)"}
        aria-label="Next match"
      >
        <Icon name="chevron-down" size={15} />
      </button>

      <button
        class="find-bar__toggle"
        class:find-bar__toggle--on={searchMatchCase}
        onclick={() => {
          searchMatchCase = !searchMatchCase;
          runSearch();
        }}
        use:tooltip={"Match case"}
        aria-pressed={searchMatchCase}
      >
        Aa
      </button>
      <button
        class="find-bar__toggle"
        class:find-bar__toggle--on={searchWholeWord}
        onclick={() => {
          searchWholeWord = !searchWholeWord;
          runSearch();
        }}
        use:tooltip={"Whole word"}
        aria-pressed={searchWholeWord}
      >
        ab|
      </button>

      <div class="find-bar__spacer"></div>

      <button
        class="oa-icon-btn oa-icon-btn--sm"
        class:oa-icon-btn--selected={showSearchResults}
        onclick={() => (showSearchResults = !showSearchResults)}
        use:tooltip={"Show all results"}
        aria-label="Toggle results list"
      >
        <Icon name="list-tree" size={15} />
      </button>
      <button class="oa-icon-btn oa-icon-btn--sm" onclick={closeSearch} use:tooltip={"Close (Esc)"} aria-label="Close find bar">
        <Icon name="x" size={15} />
      </button>
    </div>
  {/if}

  <!-- Shown from two documents up: with one open, the path bar below
       already says which file this is, and an always-present tab strip
       would just be a row of chrome doing nothing. -->
  {#if tabs.length > 1}
    <div class="tab-bar" role="tablist">
      {#each tabs as tab, i (tab.filePath)}
        <div class="tab" class:tab--active={i === activeTabIndex}>
          <button
            class="tab__label"
            role="tab"
            aria-selected={i === activeTabIndex}
            onclick={() => switchToTab(i)}
            use:tooltip={tab.filePath}
          >
            <span class="tab__name">{tab.filePath.split(/[/\\]/).pop()}</span>
            {#if tab.doc.is_dirty}<span class="tab__dirty" aria-label="Unsaved changes">●</span>{/if}
          </button>
          <button class="tab__close" onclick={() => closeTab(i)} aria-label="Close tab">
            <Icon name="x" size={12} />
          </button>
        </div>
      {/each}
    </div>
  {/if}

  {#if filePath}
    <div class="path-bar">
      <Icon name="file-pen" size={12} />
      <span class="path-bar__text">{filePath}</span>
      {#if doc?.is_dirty}<span class="oa-tag path-bar__dirty">Unsaved</span>{/if}
      {#if doc}
        <!-- Phone only. On a desktop these live in the topbar, where
             there is room for them; on a phone there is not, and this
             strip is the only other place they belong next to the name
             of the document they describe. Hidden above 720px by the
             rule on `.path-bar__meta`. -->
        <div class="path-bar__meta">
          <span class="path-bar__pages">{currentPage + 1} / {doc.page_count}</span>
          <ZoomControl
            {zoom}
            canZoomOut={zoom > MIN_ZOOM}
            canZoomIn={zoom < MAX_ZOOM}
            onZoomOut={zoomOut}
            onZoomIn={zoomIn}
            onReset={zoomReset}
          />
        </div>
      {/if}
    </div>
  {/if}

  {#if error}
    <div class="banner">
      <Icon name="triangle-alert" size={15} />
      <p>{error}</p>
    </div>
  {/if}

  <div class="body">
    {#if doc}
      <aside class="rail">
        {#each TOOL_GROUPS as group, g (group.name)}
          <!-- The heading is decorative on a phone, where the rail is a
               horizontal strip and a left border stands in for it. -->
          <p class="rail__heading">{group.name}</p>
          <div class="rail__group" class:rail__group--later={g > 0}>
            {#each group.tools as tool (tool.id)}
              <button
                class="oa-rail-btn rail__tool"
                class:oa-rail-btn--selected={activeTool === tool.id}
                onclick={() => (activeTool = tool.id)}
                use:tooltip={toolHint(tool.id) ?? tool.label}
                aria-label={tool.label}
              >
                <Icon name={tool.icon} size={18} />
                <span class="rail__label">{tool.label}</span>
              </button>
            {/each}
          </div>
        {/each}
        <div class="rail__spacer"></div>
        <div class="rail__colors">
          {#each PRESET_COLORS as preset (preset.label)}
            <button
              class="swatch"
              class:swatch--selected={color === preset.value}
              style="background: rgb({preset.value[0] * 255}, {preset.value[1] * 255}, {preset.value[2] * 255});"
              aria-label={preset.label}
              use:tooltip={preset.label}
              onclick={() => (color = preset.value)}
            ></button>
          {/each}
        </div>
      </aside>

      <Viewer
        handle={doc.handle}
        pageSizes={doc.page_sizes}
        {zoom}
        {activeTool}
        {color}
        busy={mutationBusy}
        onCreateAnnotation={handleCreateAnnotation}
        onRedact={handleRedact}
        onToolClick={handleToolClick}
        onCreateField={handleCreateField}
        onPlaceSignature={handlePlaceSignature}
        onMoveObject={handleMoveObject}
        onZoom={(next) => (zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, next)))}
        {selection}
        onSelectText={handleSelectText}
        {formFields}
        onFillField={handleFillFieldInline}
        {focusField}
        {searchHits}
        activeHitIndex={searchActiveIndex}
        {scrollToPage}
        onCurrentPageChange={(pageIndex) => (currentPage = pageIndex)}
      />
      {#if showOutline}
        <OutlinePanel
          entries={outline}
          loading={outlineLoading}
          {currentPage}
          onGoToPage={goToPage}
        />
      {/if}
      {#if showSearch && showSearchResults}
        <SearchPanel
          hits={searchHits}
          activeIndex={searchActiveIndex}
          busy={searchBusy}
          searched={searchRan}
          truncated={searchTruncated}
          onSelect={(index) => (searchActiveIndex = index)}
        />
      {/if}
      {#if showComments}
        <CommentsPanel {annotations} loading={annotationsLoading} />
      {/if}
      {#if showPages}
        <PagesPanel
          handle={doc.handle}
          pageSizes={doc.page_sizes}
          busy={pagesBusy || mutationBusy}
          onRotate={handleRotate}
          onDelete={handleDelete}
          onMove={handleMove}
          onCrop={handleCrop}
          onExtractSelected={handleExtractSelected}
          onMerge={handleMerge}
        />
      {/if}
      {#if showForms}
        <FormsPanel fields={formFields} busy={formsBusy || mutationBusy} onFill={handleFillForm} />
      {/if}
      {#if showSignatures}
        <SignaturesPanel
          armedId={armedSignatureId}
          onArm={(id) => (armedSignatureId = id)}
          onNew={() => (showSignaturePad = true)}
        />
      {/if}
    {:else}
      <div class="empty-state">
        <BrandMark variant="monogram" size={40} />
        <p>Open a PDF to get started.</p>
        <button class="oa-btn oa-btn--primary" onclick={pickAndOpen}>
          <Icon name="folder-open" size={15} />
          Open PDF…
        </button>

        <!-- The file you want next is usually one you had open
             recently, and the picker made you go and find it again.
             Absent rather than empty when there is nothing to show: a
             "Recent" heading over blank space on first run is a promise
             about a feature nobody asked about yet. -->
        {#if recents.length > 0}
          <div class="recents">
            <div class="recents__head">
              <span class="recents__title">Recent</span>
              <button class="recents__clear" onclick={handleClearRecents}>Clear</button>
            </div>
            {#each recents as entry (entry.id)}
              <div class="recent">
                <button
                  class="recent__open"
                  onclick={() => handleOpenRecent(entry.id)}
                  disabled={recentBusy !== null}
                >
                  <Icon name={recentBusy === entry.id ? "loader-circle" : "file-pen"} size={15} spin={recentBusy === entry.id} />
                  <span class="recent__name">{entry.name}</span>
                  <span class="recent__when">{describeWhen(entry.openedAt, now)}</span>
                </button>
                <button
                  class="recent__forget"
                  onclick={() => handleForgetRecent(entry.id)}
                  use:tooltip={"Remove from this list"}
                  aria-label={`Remove ${entry.name} from the recent list`}
                >
                  <Icon name="x" size={13} />
                </button>
              </div>
            {/each}
          </div>
        {/if}
        <!-- Here rather than in the topbar: this is the one moment
             someone is deciding whether to keep the thing, and it is
             gone the instant a document is open. -->
        <InstallPrompt />
      </div>
    {/if}
  </div>
</main>

<style>
  /* ---- Tabs ---- */
  .tab-bar {
    flex: 0 0 auto;
    display: flex;
    align-items: stretch;
    gap: 2px;
    padding: var(--space-1) var(--space-2) 0;
    background: var(--surface-card);
    border-bottom: var(--border-width) solid var(--border-hairline);
    overflow-x: auto;
  }

  .tab {
    display: flex;
    align-items: center;
    max-width: 16rem;
    border: var(--border-width) solid transparent;
    border-bottom: 0;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    background: transparent;
    transition: var(--transition-control);
  }
  .tab:hover {
    background: var(--surface-hover);
  }
  .tab--active {
    background: var(--bg-page);
    border-color: var(--border-hairline);
  }

  .tab__label {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
    border: 0;
    background: transparent;
    padding: 5px var(--space-2);
    cursor: pointer;
    font: var(--type-ui);
    color: var(--text-muted);
  }
  .tab--active .tab__label {
    color: var(--text-strong);
  }

  .tab__name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tab__dirty {
    flex: 0 0 auto;
    color: var(--warning-fg);
    font-size: 10px;
    line-height: 1;
  }

  .tab__close {
    display: flex;
    align-items: center;
    border: 0;
    background: transparent;
    padding: 4px;
    margin-right: 2px;
    border-radius: var(--radius-xs);
    cursor: pointer;
    color: var(--text-faint);
  }
  .tab__close:hover {
    background: var(--surface-hover);
    color: var(--text-strong);
  }

  /* ---- Find bar ---- */
  .find-bar {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-3);
    background: var(--surface-card);
    border-bottom: var(--border-width) solid var(--border-hairline);
    color: var(--text-muted);
  }

  .find-bar__input {
    flex: 0 1 22rem;
    min-width: 8rem;
    height: var(--control-h-sm);
    padding: 0 var(--space-2);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-sm);
    background: var(--bg-page);
    color: var(--text-strong);
    font: var(--type-ui);
  }

  .find-bar__input:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .find-bar__count {
    min-width: 6.5rem;
    font: var(--type-caption);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .find-bar__toggle {
    height: var(--control-h-sm);
    min-width: var(--control-h-sm);
    padding: 0 6px;
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    font: var(--type-caption);
    cursor: pointer;
    transition: var(--transition-control);
  }

  .find-bar__toggle:hover {
    background: var(--surface-hover);
  }

  .find-bar__toggle--on {
    background: var(--surface-selected);
    border-color: var(--border-hairline);
    color: var(--text-strong);
  }

  .find-bar__spacer {
    flex: 1;
  }

  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  /* ---- Topbar — 52px, matches --topbar-h ---- */
  .topbar {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--topbar-h);
    padding: 0 var(--space-3);
    background: var(--surface-card);
    border-bottom: var(--border-width) solid var(--border-hairline);
    flex-wrap: wrap;
  }

  .topbar__group {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  /* ---- Document tools ----
     Their own band under the topbar rather than seventeen unlabelled
     icons crowded into its right-hand end. Named, and in named groups:
     the difference between Flatten and Compress is not something an
     icon can carry, and both are one click from irreversible.

     It scrolls sideways rather than wrapping. Wrapping moves every tool
     when the window is resized, and a toolbar whose contents change
     position has to be re-read each time. */
  .tools {
    flex: 0 0 auto;
    display: flex;
    align-items: flex-start;
    gap: 0;
    padding: var(--space-2) var(--space-3);
    background: var(--surface-card);
    border-bottom: var(--border-width) solid var(--border-hairline);
    overflow-x: auto;
    scrollbar-width: thin;
  }

  .tools__section {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 var(--space-3);
  }

  .tools__section + .tools__section {
    border-left: var(--border-width) solid var(--border-hairline);
  }

  .tools__section:first-child {
    padding-left: 0;
  }

  .tools__heading {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
    padding-left: 2px;
  }

  .tools__group {
    display: flex;
    align-items: flex-start;
    gap: 2px;
  }

  /* Icon over name. The names are what the band is for. */
  .tools :global(.oa-icon-btn) {
    width: auto;
    min-width: 54px;
    height: auto;
    flex-direction: column;
    justify-content: flex-start;
    gap: 3px;
    padding: 5px 6px 4px;
  }

  .tools__label {
    font-size: 10px;
    line-height: 1.1;
    white-space: nowrap;
  }

  /* Phone only — the media query at the end of this file turns it on.
     Hidden here rather than only shown there, or a desktop gets a button
     that toggles a sheet it never renders. */
  .topbar__more {
    display: none;
  }

  .topbar__spacer {
    flex: 1;
  }

  .topbar__meta {
    white-space: nowrap;
  }

  /* A dot, in the corner of the account button, when there is a session.
     Small on purpose: it answers "am I signed in" for anyone who looks,
     without becoming something to look at. */
  .account-btn {
    position: relative;
  }

  .account-btn--in::after {
    content: "";
    position: absolute;
    right: 3px;
    bottom: 3px;
    width: 7px;
    height: 7px;
    border-radius: var(--radius-full);
    background: var(--brand, #10b981);
    box-shadow: 0 0 0 2px var(--surface-card);
  }

  .topbar__pill-btn {
    border: 0;
    cursor: pointer;
    transition: var(--transition-control);
  }
  .topbar__pill-btn:hover {
    filter: brightness(0.97);
  }

  /* ---- Secondary strip: the open file's path ---- */
  .path-bar {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 26px;
    padding: 0 var(--space-3);
    background: var(--bg-subtle);
    border-bottom: var(--border-width) solid var(--border-hairline);
    color: var(--text-faint);
  }
  .path-bar :global(.oa-icon) {
    flex: 0 0 auto;
  }
  .path-bar__text {
    font: var(--type-caption);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Phone only — see the markup. Kept out of the way of a desktop,
     which shows the same two things in the topbar. */
  .path-bar__meta {
    display: none;
  }

  .path-bar__dirty {
    flex: 0 0 auto;
    color: var(--warning-fg);
    border-color: color-mix(in oklab, var(--warning-fg) 40%, transparent);
  }

  .banner {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--danger-bg);
    color: var(--danger-fg);
    border-bottom: var(--border-width) solid var(--border-hairline);
  }
  .banner p {
    font: var(--type-caption);
    font-size: var(--text-sm);
  }

  .body {
    flex: 1;
    display: flex;
    overflow: hidden;
  }

  /* ---- Tool rail ----
     Named tools in named groups. It was a 56px column of sixteen glyphs,
     which is only usable by people who already know which picture means
     Redact — and Redact is the one that destroys content. */
  .rail {
    flex: 0 0 auto;
    width: var(--rail-w);
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 2px;
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-2);
    background: var(--bg-subtle);
    border-right: var(--border-width) solid var(--border-hairline);
    overflow-y: auto;
  }

  .rail__heading {
    margin: var(--space-3) 0 2px;
    padding: 0 6px;
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
  }

  .rail__heading:first-child {
    margin-top: 0;
  }

  .rail__group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* Icon then name, left-aligned so the names form a column the eye can
     run down rather than a ragged set of centred labels. */
  .rail__tool {
    width: 100%;
    height: 32px;
    justify-content: flex-start;
    gap: var(--space-2);
    padding: 0 6px;
  }

  .rail__label {
    font: var(--type-body-sm, 12px/1.2 inherit);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rail__spacer {
    flex: 1;
    min-height: var(--space-2);
  }

  .rail__colors {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: var(--space-2) 6px 4px;
    border-top: var(--border-width) solid var(--border-hairline);
  }

  .swatch {
    width: 18px;
    height: 18px;
    flex: 0 0 auto;
    border-radius: var(--radius-full);
    padding: 0;
    border: 2px solid var(--surface-card);
    box-shadow: 0 0 0 1px var(--border-hairline);
    cursor: pointer;
  }

  .swatch--selected {
    box-shadow: 0 0 0 1.5px var(--border-focus);
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-4);
  }

  .empty-state p {
    font: var(--type-body);
    color: var(--text-muted);
  }

  /* Sized to the list rather than to the screen: a full-width column on
     a 27-inch display would make six filenames look like a database. */
  .recents {
    width: min(420px, 100%);
    margin-top: var(--space-2);
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

  /* Visible only on hover or focus: six of these down the side of the
     list is a column of ✕ competing with the filenames, and removing a
     row is the rarer thing to want by far. */
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

  /* ---------------------------------------------------------------- *
   * Phones.
   *
   * The desktop layout puts 24 buttons in the topbar and a permanent
   * 56px rail down the left. On a 390px screen that wrapped over the
   * path bar, ran a second row off the right edge, and left a sliver
   * for the page — the thing anyone actually came to look at.
   *
   * Nothing is duplicated to fix it. The same buttons, handlers and
   * disabled states are simply drawn somewhere else: the document tools
   * become a sheet, the tool rail becomes a thumb-reachable bar along
   * the bottom, and side panels become full-screen. One set of markup,
   * two arrangements.
   * ---------------------------------------------------------------- */
  @media (max-width: 720px) {
    /* The page gets the room — but 2px between controls made a row of
       buttons read as one undifferentiated strip, and put their tap
       targets closer together than a fingertip can resolve. The space
       comes from the wordmark instead: the app is already open, and the
       empty state carries the mark in the middle of the screen where
       there is room for it. */
    .topbar {
      gap: var(--space-2);
      padding: 0 var(--space-3);
      height: 56px;
    }

    .topbar :global(.oa-brandmark) {
      display: none;
    }

    /* 40px, up from 28: the smallest thing worth aiming a thumb at. */
    .topbar :global(.oa-icon-btn) {
      width: 40px;
      height: 40px;
    }

    .topbar :global(.oa-btn) {
      height: 40px;
    }

    .topbar__more {
      display: inline-flex;
    }

    /* The document tools, and zoom, as a sheet above the tool bar rather
       than a row that cannot fit. `position: fixed` rather than a
       separate component: the buttons keep their identity, so a disabled
       Save is still disabled here. */
    .topbar__group--overflow {
      position: fixed;
      left: 0;
      right: 0;
      bottom: var(--mobile-toolbar-h);
      z-index: 60;
      display: none;
      flex-wrap: wrap;
      justify-content: flex-start;
      gap: var(--space-2);
      padding: var(--space-4);
      background: var(--surface-card);
      border-top: var(--border-width) solid var(--border-hairline);
      box-shadow: var(--shadow-xl);
      max-height: 55dvh;
      overflow-y: auto;
    }

    .topbar__group--overflow.is-open {
      display: flex;
    }

    /* The band becomes the sheet: sections stacked instead of side by
       side, names beside the icons rather than under them, and targets a
       finger can hit — 44px is the smallest thing Apple and Google both
       consider tappable. */
    .topbar__group--overflow {
      flex-direction: column;
      align-items: stretch;
      gap: 0;
      padding: var(--space-4);
      overflow-x: hidden;
      /* Not wrap. A column that is allowed to wrap turns "taller than
         max-height" into a second column running off the right edge —
         which is where SAVE & PROTECT went. It scrolls instead. */
      flex-wrap: nowrap;
    }

    .tools__section,
    .tools__section:first-child {
      padding: 0;
      margin-top: var(--space-3);
      border-left: none;
    }

    .tools__section:first-child {
      margin-top: 0;
    }

    .tools__group {
      flex-wrap: wrap;
      gap: var(--space-2);
    }

    .tools :global(.oa-icon-btn) {
      min-width: 44px;
      height: 44px;
      flex-direction: row;
      align-items: center;
      justify-content: flex-start;
      gap: var(--space-2);
      padding: 0 10px;
    }

    .tools__label {
      font-size: 12px;
    }

    /* Tools along the bottom, where a thumb reaches. Horizontal because
       there are seventeen of them and a phone has width to scroll, not
       height to spare. */
    .body {
      flex-direction: column-reverse;
    }

    .rail {
      width: 100%;
      height: var(--mobile-toolbar-h);
      flex-direction: row;
      align-items: center;
      gap: 2px;
      padding: 0 var(--space-2);
      overflow-x: auto;
      overflow-y: hidden;
      border-right: none;
      border-top: var(--border-width) solid var(--border-hairline);
      /* Momentum scrolling, and no scrollbar eating a phone's height. */
      -webkit-overflow-scrolling: touch;
      scrollbar-width: none;
    }

    .rail::-webkit-scrollbar {
      display: none;
    }

    /* Icon over name, in a strip that scrolls. Wide enough for the
       longest name that fits at this size; the rest ellipsise, and the
       tooltip is still there. */
    .rail__tool {
      flex: 0 0 auto;
      width: 62px;
      height: 52px;
      flex-direction: column;
      justify-content: center;
      gap: 1px;
      padding: 0 2px;
    }

    .rail__label {
      font-size: 9px;
      line-height: 1.1;
      max-width: 100%;
    }

    /* No room for headings across a phone. A rule between groups says
       the same thing in the space available. */
    .rail__heading {
      display: none;
    }

    .rail__group {
      flex-direction: row;
      align-items: center;
    }

    .rail__group--later {
      margin-left: var(--space-2);
      padding-left: var(--space-2);
      border-left: var(--border-width) solid var(--border-hairline);
    }

    .rail__spacer {
      display: none;
    }

    .rail__colors {
      flex-direction: row;
      gap: 6px;
      padding-left: var(--space-2);
      border-top: none;
      border-left: var(--border-width) solid var(--border-hairline);
    }


    /* A side panel beside a 390px page is not a panel, it is a column an
       inch wide. Full screen, above the sheet. */
    .body :global(aside:not(.rail)),
    .body :global(.panel) {
      position: fixed;
      inset: var(--topbar-h) 0 var(--mobile-toolbar-h) 0;
      width: auto;
      max-width: none;
      z-index: 55;
      border-left: none;
      overflow-y: auto;
    }

    /* The file strip carries what the topbar cannot: which page you are
       on and what the zoom is. Taller than the desktop's 26px hairline
       because it holds controls now, and because the name of the
       document was previously jammed against the toolbar above it. */
    .path-bar {
      height: auto;
      min-height: 44px;
      padding: 6px var(--space-3);
      gap: var(--space-2);
    }

    .path-bar__meta {
      display: flex;
      align-items: center;
      gap: 2px;
      /* Pushed right, away from the name, which takes what it needs of
         the rest and ellipsises the remainder. */
      margin-left: auto;
      flex: 0 0 auto;
    }

    .path-bar__pages {
      font: var(--type-caption);
      font-variant-numeric: tabular-nums;
      white-space: nowrap;
      padding-right: var(--space-1, 4px);
    }

    /* Both of these moved to the file strip rather than disappearing.
       The topbar copies stay hidden: they were briefly in the tools
       sheet, where they sat underneath the document tools at the same
       fixed position and could not be tapped at all. */
    .topbar__group--zoom,
    .topbar__meta {
      display: none;
    }
  }
</style>