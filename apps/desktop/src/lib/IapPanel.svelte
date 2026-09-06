<script lang="ts">
  // Buying credits inside the iOS app.
  //
  // Replaces <openapps-buy> there, and is not an alternative to it:
  // App Store Review Guideline 3.1.1 requires digital content used in an
  // app to be sold through in-app purchase, so a card checkout inside this
  // shell is not a second option, it is grounds for rejection.
  //
  // The sequence every path here follows, and the reason this file is more
  // than a button:
  //
  //   1. StoreKit takes the money and hands back a signed transaction.
  //   2. The server verifies it and grants the credits.
  //   3. Only then is the transaction *finished*.
  //
  // Step 3 last is the whole design. StoreKit re-delivers an unfinished
  // transaction on every launch until it is finished, which is what makes a
  // purchase survive a crash, a dead network or a force-quit between steps
  // 1 and 2. Finishing early throws that away and leaves someone charged
  // for credits nobody granted, with no record left to retry from.
  //
  // `recover()` is the other half: it runs on mount rather than behind a
  // "Restore purchases" button, because someone whose payment went through
  // should not have to know that word.
  import { collect } from "$lib/iap";
  import { nativeShell, type NativeProduct, type NativePurchase } from "$lib/native";
  import { showToast } from "$lib/toast.svelte";

  const shell = nativeShell();

  let products = $state<NativeProduct[]>([]);
  let loading = $state(true);
  let busy = $state<string | null>(null);
  let problem = $state<string | null>(null);

  $effect(() => {
    if (!shell) return;
    let cancelled = false;

    void (async () => {
      try {
        const listed = await shell.products();
        if (!cancelled) products = listed;
      } catch (e) {
        if (!cancelled) problem = message(e);
      } finally {
        if (!cancelled) loading = false;
      }
    })();

    // A purchase StoreKit delivers on its own: an interrupted one
    // completing, an Ask to Buy approval, or one made on another device.
    // The sweep for anything left owing from an earlier run happens at
    // startup instead (see +page.svelte) — waiting until someone opens
    // this panel would leave credits they paid for uncollected until they
    // went looking for them.
    const stop = shell.on("receipt", (receipt) => void grant(receipt, { quiet: true }));

    return () => {
      cancelled = true;
      stop();
    };
  });

  async function buy(product: NativeProduct): Promise<void> {
    if (!shell || busy) return;
    busy = product.id;
    problem = null;
    try {
      const outcome = await shell.purchase(product.id);
      if (outcome.status === "cancelled") return;
      if (outcome.status === "pending") {
        showToast("Waiting for approval. The credits will arrive once it is granted.", {
          title: "Purchase pending",
        });
        return;
      }
      await grant(outcome, { quiet: false });
    } catch (e) {
      problem = message(e);
    } finally {
      busy = null;
    }
  }

  /** Steps 2 and 3, which `collect` keeps in that order. */
  async function grant(receipt: NativePurchase, opts: { quiet: boolean }): Promise<void> {
    if (!shell) return;
    const result = await collect(receipt, shell);

    if (result.ok) {
      if (!result.alreadyCredited) {
        showToast(`${result.credits.toLocaleString()} credits added.`, { title: "Thank you" });
      }
      return;
    }

    // The transaction is deliberately still unfinished here: a receipt the
    // server has not honoured is the customer's only evidence they paid.
    if (result.kind === "unauthorized") {
      problem = "Sign in again to collect these credits — the purchase is safe until you do.";
    } else if (result.kind === "retry") {
      problem = "The purchase went through but the credits have not arrived yet. Reopen the app to try again.";
    } else {
      problem = result.message;
    }
    if (opts.quiet && problem) {
      showToast(problem, { tone: "warning", title: "Purchase not yet credited" });
    }
  }

  function message(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }
</script>

{#if shell}
  <section class="iap">
    <h3 class="iap__title">Buy credits</h3>
    {#if loading}
      <p class="iap__note">Loading…</p>
    {:else if products.length === 0}
      <p class="iap__note">
        Credit packs are unavailable right now. Check your connection and reopen the app.
      </p>
    {:else}
      <ul class="iap__list">
        {#each products as product (product.id)}
          <li class="iap__row">
            <div class="iap__text">
              <span class="iap__name">{product.name}</span>
              <span class="iap__desc">{product.description}</span>
            </div>
            <button
              class="oa-btn oa-btn--primary"
              disabled={busy !== null}
              onclick={() => buy(product)}
            >
              {busy === product.id ? "…" : product.price}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if problem}
      <p class="iap__problem" role="alert">{problem}</p>
    {/if}
  </section>
{/if}

<style>
  .iap {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .iap__title {
    font: var(--type-label);
    color: var(--text-muted);
    margin: 0;
  }

  .iap__list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .iap__row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .iap__text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .iap__name {
    font: var(--type-body);
  }

  .iap__desc {
    font: var(--type-caption);
    color: var(--text-muted);
  }

  .iap__note,
  .iap__problem {
    font: var(--type-caption);
    color: var(--text-muted);
    margin: 0;
  }

  .iap__problem {
    color: var(--danger, #d33);
  }
</style>
