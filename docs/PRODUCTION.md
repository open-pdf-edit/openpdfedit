# Running openpdfedit.com in production

Four hostnames, one server (`104.36.65.54`), and one thing that is not
this server at all:

| host | what it is | served by |
|---|---|---|
| `openpdfedit.com` | the marketing site | static files from `site/` |
| `www.openpdfedit.com` | redirect to the above | Caddy |
| `app.openpdfedit.com` | the web app | static files from `apps/webapp/dist/` |
| `auth.openpdfedit.com` | sign-in | reverse proxy to openapps-server on `:8080` |
| `gateway.openapps.network` | the credit charge for the watermark unlock | already running; nothing to add |

`auth.openpdfedit.com` is a second name for the machine that already
answers as `accounts.openapps.network` — same process, same database,
same accounts. Sessions are bearer tokens rather than cookies, so
nothing about identity is scoped to a hostname and a second name changes
nothing functionally. It exists so signing in doesn't show a stranger's
domain to someone who has only seen "OpenPdfEdit".

Everything below marked **you** needs your server or your registrar.
Everything else is already in this repository.

---

## 1. DNS — one record is missing

What you have set is right:

```
A      @      104.36.65.54
CNAME  auth   accounts.openapps.network.
CNAME  www    openpdfedit.com.
```

**You:** add the web app's name, which has no record yet:

```
CNAME  app    openpdfedit.com.
```

Confirm all four resolve to the same address before going further —
Caddy cannot get a certificate for a name that doesn't resolve:

```sh
for h in openpdfedit.com www.openpdfedit.com auth.openpdfedit.com app.openpdfedit.com; do
  printf '%-26s %s\n' "$h" "$(dig +short "$h" | tail -1)"
done
```

All four must print `104.36.65.54`.

---

## 2. Caddy — four site blocks

**You**, on the server. This adds to the existing Caddyfile; leave the
`accounts.openapps.network` / `app.openapps.network` /
`gateway.openapps.network` blocks exactly as they are.

```sh
sudo nano /etc/caddy/Caddyfile
```

Append:

```caddy
openpdfedit.com {
	root * /var/www/openpdfedit
	file_server
	encode gzip zstd
}

www.openpdfedit.com {
	redir https://openpdfedit.com{uri} permanent
}

app.openpdfedit.com {
	root * /var/www/openpdfedit-app
	file_server
	encode gzip zstd

	# The two WebAssembly binaries are ~9 MB uncompressed and about a
	# third of that over the wire, so compression is not cosmetic here.
	# They are also content-addressed by the service worker's cache
	# name, which is why they can be cached hard while index.html and
	# the service worker itself must not be.
	@immutable path /app/immutable/* /fonts/*
	header @immutable Cache-Control "public, max-age=31536000, immutable"

	@volatile path / /index.html /service-worker.js /manifest.webmanifest
	header @volatile Cache-Control "no-cache"

	# Single-page app: every unknown path is the app's own route, not a
	# missing file. Without this, a reload on /login 404s.
	try_files {path} /index.html
}

auth.openpdfedit.com {
	reverse_proxy 127.0.0.1:8080
}
```

Those are tabs, not spaces — Caddy cares. Then:

```sh
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

Certificates are issued automatically, within a few seconds of the
reload. `sudo journalctl -u caddy -n 50` if a name doesn't come up.

> Using nginx instead? Mirror the existing `accounts` block for `auth`,
> add two static `server` blocks with the same `Cache-Control` split and
> a `try_files $uri /index.html;` on the app one, and run `certbot
> --nginx -d openpdfedit.com -d www.openpdfedit.com -d
> app.openpdfedit.com -d auth.openpdfedit.com`.

---

## 3. Let openapps-server accept the new origins

Without this, Google sign-in fails: the server validates the `return_to`
it is handed against a list, and both new origins are absent from it.
This is the single most likely thing to be forgotten, and the symptom is
a sign-in that dead-ends *after* the Google screen rather than an error
you'd notice earlier.

**You**, in `deploy/prod.env` on the server — extend the existing value,
don't replace it:

```sh
OPENAPPS_SERVER_ALLOWED_ORIGINS=<what is already there>,https://auth.openpdfedit.com,https://app.openpdfedit.com
```

Then restart the server the way you normally do (`deploy/run.sh`, or
`docker restart openapps-server`).

Note `https://openpdfedit.com` is deliberately *not* in that list: the
marketing site never signs anyone in. Only the app does.

---

## 4. Register OpenPdfEdit as a paid app

The watermark unlock charges 1,000 credits through
`gateway.openapps.network`. The gateway holds one app key per product —
that is how the ledger knows which product earned the revenue — and
there isn't one for OpenPdfEdit yet.

**You:** mint the key (needs your admin token):

```sh
curl -X POST https://accounts.openapps.network/v1/admin/apps \
  -H "Authorization: Bearer <admin token>" \
  -H 'content-type: application/json' \
  -d '{"id":"openpdfedit","name":"OpenPdfEdit"}'
```

The `id` must be exactly `openpdfedit` — the gateway derives both the
ledger reason (`openpdfedit_supporter_unlock`) and the idempotency key
from it, and the app checks the entitlement under that same string.

Add the key it returns to `deploy/gateway.env`:

```sh
OPENAPPS_KEY_OPENPDFEDIT=oa_live_...
```

And add the web app's origin to the gateway's CORS list, since the
unlock is called from the browser:

```sh
GATEWAY_ALLOWED_ORIGINS=<what is already there>,https://app.openpdfedit.com
```

Restart the gateway (`deploy/run-gateway.sh`). It logs the app ids it can
bill for at startup — check `openpdfedit` is among them:

```sh
docker logs openapps-gateway 2>&1 | grep -i apps
```

A missing key doesn't fail at boot. It fails at the first user's unlock,
which is the worst possible time to find out.

---

## 5. Deploy the two static sites

From a checkout of this repository, on your machine:

```sh
# the web app — a real build, not a copy of the source
npm --prefix apps/webapp run build

# ship both
rsync -av --delete site/              root@104.36.65.54:/var/www/openpdfedit/
rsync -av --delete apps/webapp/dist/  root@104.36.65.54:/var/www/openpdfedit-app/
```

`--delete` matters on the app: its JavaScript filenames are content
hashes, so without it every deploy leaves the previous build's chunks
behind forever.

**Redeploying the app later:** the service worker's cache name is a
digest of the build, so a rebuild that changed something gets a new name
and every returning visitor picks it up on their next load. A rebuild
that changed nothing keeps the old name and costs them nothing. There is
no version number to remember to bump.

---

## 6. Check it

```sh
curl -sI https://openpdfedit.com            | head -1   # 200
curl -sI https://www.openpdfedit.com        | head -1   # 301
curl -sI https://app.openpdfedit.com        | head -1   # 200
curl -sI https://app.openpdfedit.com/pdfium.wasm | grep -i content-type   # application/wasm
curl -s  https://auth.openpdfedit.com/healthz                             # ok
curl -sI https://app.openpdfedit.com/login  | head -1   # 200, not 404
```

That last one is the SPA fallback. If it 404s, `try_files` is missing and
sign-in will break on the redirect back from Google.

Then, in a browser:

1. Open `https://app.openpdfedit.com` and edit a PDF. No account needed.
2. Account → **Sign in**. A popup opens on `auth.openpdfedit.com`.
   Complete it; the popup closes and the main window shows you signed in
   without reloading.
3. Click **Watermark**. Signed out it offers sign-in; signed in and
   unpaid it offers the 1,000-credit unlock; after unlocking it opens
   the tool and keeps opening it.

---

## Two seams that are cosmetic, and stay

Both come from openapps-server building these strings once at startup
from its own `public_url`, rather than per request. Neither is a
security property — the server never checks either string against the
actual request — and fixing them means restructuring the backend for
branding alone.

- **Google sign-in visibly bounces through `accounts.openapps.network`**
  on the OAuth callback hop, even though it started on
  `auth.openpdfedit.com`.
- **A wallet signature prompt names `accounts.openapps.network`.** A
  wallet that cross-checks a SIWE message's domain against the page's
  origin could flag this. Nostr sign-in has neither issue.

---

## What breaks, and where to look first

| symptom | cause |
|---|---|
| sign-in dead-ends after the Google screen | §3 — the origin isn't in `OPENAPPS_SERVER_ALLOWED_ORIGINS` |
| reloading `/login` gives a 404 | §2 — `try_files` missing on the app block |
| unlock fails with a network error, nothing in the gateway log | §4 — origin missing from `GATEWAY_ALLOWED_ORIGINS`; the browser blocks it before the request is sent |
| unlock returns 500 | §4 — no `OPENAPPS_KEY_OPENPDFEDIT` in the gateway's environment |
| the app loads yesterday's build | a stale service worker; hard-reload once, then check §5's `--delete` actually ran |
| a certificate never appears | §1 — that name has no DNS record, or it doesn't point here yet |
