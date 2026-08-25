# The Supporter tools, and where their source lives

Watermark and OCR are unlocked by redeeming 1,000 credits. Their
implementations are moving to a private repository so a fork cannot ship
them for free; everything else about OpenPdfEdit stays open.

Status: **the private repository exists and holds the code. The public
crates have not been emptied yet** — that is the remaining step, on the
`premium-split` branch. Until it lands, the implementations are still in
this repository.

## What moves and what does not

| stays public | moves private |
|---|---|
| the gate: `SupporterGate.svelte`, `$lib/openapps` — the entitlement check, the unlock, the price | the tiling maths (`openpdfedit-watermark`) |
| the option types, their validation, and the errors callers handle | the recognition and text-layer code (`openpdfedit-ocr`) |
| every other tool | `WatermarkPanel.svelte`, `ocr-browser.ts` |

The gate is public on purpose. What a user is charged, and on what
terms, should be readable by that user. Only the feature is private.

## What this does and does not buy

It stops a public fork from shipping working paid features with no
effort. That is all. Every render happens on the user's own machine, and
anyone with a built binary can disassemble it — this raises the bar
against casual copying, and should never be described as protection.

**It applies to v0.1.7 onward and not one version earlier.** The full
implementations shipped in v0.1.1 through v0.1.6 under MIT OR Apache-2.0.
Those grants are irrevocable: anyone holding a copy of those releases may
use, modify and redistribute the code for ever. Deleting the old releases
removes the download, not the licence, and not the git history.

## The constraint that shapes the design

The obvious approach — an optional Cargo dependency on a path inside the
submodule — does not work. Cargo reads every path dependency's manifest
during resolution, **before** features are considered, so a missing path
fails the build even when the feature is off:

```
failed to read `vendor-private/not-checked-out/Cargo.toml`
Caused by: No such file or directory (os error 2)
```

Verified directly, not assumed. So the public repository cannot simply
point a dependency at the submodule and hope it degrades.

Instead the crates stay where they are and their **contents** are
swapped:

- `crates/openpdfedit-{watermark,ocr}/src/lib.rs` in this repository hold
  the types, the validation and an entry point that returns
  `NotIncluded`.
- The private repository holds the same crates, complete.
- `scripts/use-premium.sh` copies the private sources over the public
  ones before an official build.

A checkout without the submodule builds, runs, and shows both buttons and
their unlock prompt exactly as an official build does. Applying either
tool then reports that the implementation is absent. That is deliberate:
the two builds should be indistinguishable until the moment the feature
would actually run.

## Building

```sh
git submodule update --init          # needs access to the private repo
./scripts/use-premium.sh             # copy the implementations into place
cargo build --release                # a full build
./scripts/use-premium.sh --check     # non-zero if the stubs are still in place
```

`--check` exists because the failure mode here is silent. A release built
without the submodule would ship a watermark button that takes 1,000
credits and then reports the tool is missing. **The release workflow must
run `--check` and fail if it does not pass** — that guard is not optional
and is the single most important part of this arrangement.

## Deleting the old releases

Removing v0.1.1–v0.1.5 removes the binaries and the release pages. It
does not remove the source, which stays in git history reachable from
`main`, and it does not affect the licence anyone already holds.

Worth a line in the README if it happens, so the people running those
versions are not left wondering where their build went.
