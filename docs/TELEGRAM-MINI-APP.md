# Running OpenPdfEdit as a Telegram Mini App

The web build already *is* a Mini App — a Mini App is a webview pointed at
an HTTPS URL. What N3 added is the wrapper: Telegram's bridge, its theme and
viewport, its back button, and sign-in that uses the session already on the
page.

## Set it up

1. **BotFather** → `/newbot`, then `/newapp` on that bot.
2. Give it the web app URL: `https://app.openpdfedit.com`.
3. Open it from the bot. That is the whole deployment — the same static
   build serves both the website and the Mini App.

## What changes inside Telegram

Detected by `initData` being non-empty, so an ordinary browser is untouched:

| | Behaviour |
|---|---|
| Theme | Telegram's colours mapped onto our semantic tokens |
| Viewport | `--tg-viewport-height` from `viewportStableHeight`, kept current |
| Back button | Telegram's, wired through `onBack()`; ours is hidden |
| Vertical swipes | Disabled — an editor is full of drags, and Telegram reads a downward drag as "close" |
| Sign-in | One call with `initData`; no popup, no redirect, no injected signer |

## Files stay on the device

The decision that matters, and it is a decision rather than a limitation.

**We use a file input.** Bytes go straight into WASM on the device. Nothing
is uploaded and the only size limit is what the phone holds.

**We deliberately do not use the bot.** The alternative is the user sending
a PDF to the bot, which fetches it by `file_id` — the document goes to
Telegram's servers, and the Bot API caps downloads at **20 MB**. Most
Telegram document bots work this way. It would quietly invert the one claim
this product is built on.

A consequence worth knowing rather than discovering: **"open this PDF from a
chat" is not available.** It needs the bot path, and the attachment menu
that would make it pleasant is restricted to major Telegram advertisers
anyway.

## Sign-in

`signInWithTelegram()` posts `initData` to `/v1/auth/verify` as
`{type: "telegram_init_data", init_data}`. The server verifies it two ways
(HMAC against the bot token, or Telegram's Ed25519 signature) and claims its
`hash` single-use, because `initData` carries no server nonce and is
otherwise replayable for its whole `auth_date` window.

Requires `[auth.telegram]` on the server with the `bot_id` of the same bot.
A mismatched bot id fails the signature check, which is the intended
behaviour and not a misconfiguration to work around.

## The open question: payload

The core path is **~9.4 MB** — `pdfium.wasm` 5.0 MB plus
`openpdfedit_wasm_bg.wasm` 4.4 MB — out of an 87 MB `dist`. The rest is
lazy: the six Tesseract OCR variants (~18 MB, one picked at runtime) and
`anydoc` (6.4 MB, non-PDF input only).

**This has not been measured on a real phone over cellular**, and it is the
one thing that decides whether the Mini App feels good or broken. Do that
before promoting it anywhere. If the core cannot get under ~10 MB in
practice, the better wedge is openframe's ID-photo tool — a fraction of the
payload, and a better fit for a one-shot mobile task.

The service worker already caches, so this is a first-load problem only.
