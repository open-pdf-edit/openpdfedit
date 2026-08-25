# Running openpdfedit.com in production

Four hostnames, one server (`104.36.65.54`), and one thing that is not
this server at all:

| host | what it is | served by |
|---|---|---|
| `openpdfedit.com` | the marketing site | static files from `site/` |
| `www.openpdfedit.com` | redirect to the above | nginx |
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
certbot proves control by answering a challenge on each name, so one
without a record fails the whole certificate run:

```sh
for h in openpdfedit.com www.openpdfedit.com auth.openpdfedit.com app.openpdfedit.com; do
  printf '%-26s %s\n' "$h" "$(dig +short "$h" | tail -1)"
done
```

All four must print `104.36.65.54`.

---

## 2. nginx — four server blocks, then certbot

Same shape as the `accounts` / `app` / `gateway` blocks already running,
and the same `/var/www/<name>` layout as `/var/www/opencapture`. Leave
the existing `openapps` site file alone; this is a second one.

This block was run rather than written: `nginx -t` passes on 1.18, and
serving the real `apps/webapp/dist` through it gives `application/wasm`
on the WebAssembly, a 200 on `/login` from the SPA fallback, `no-cache`
on the service worker against `immutable` on a hashed chunk, and gzip
taking `pdfium.wasm` from 5,218,943 to 2,634,767 bytes.

**You**, on the server over SSH. Paste the whole block — it writes the
file in one go, with no editor to fight:

```sh
cat > /etc/nginx/sites-available/openpdfedit <<'NGINX'
# The marketing site.
server {
    listen 80;
    server_name openpdfedit.com;
    root /var/www/openpdfedit;
    index index.html;
}

# www -> apex. A redirect rather than a second copy of the site, so
# there is one canonical URL and one place to deploy to.
server {
    listen 80;
    server_name www.openpdfedit.com;
    return 301 https://openpdfedit.com$request_uri;
}

# The web app.
server {
    listen 80;
    server_name app.openpdfedit.com;
    root /var/www/openpdfedit-app;
    index index.html;

    # nginx only learned `application/wasm` in 1.21; Ubuntu 22.04 ships
    # 1.18, where a .wasm falls back to application/octet-stream. The
    # browser then refuses the module outright —
    # WebAssembly.instantiateStreaming checks the MIME type — and the app
    # doesn't start at all.
    #
    # Done as default_type in a location, deliberately, rather than a
    # `types { application/wasm wasm; }` block: a types block in a server
    # context *replaces* the inherited map instead of extending it, which
    # would leave CSS and JS as octet-stream and break far more than it
    # fixed. default_type only applies when the map produced nothing, so
    # it is a no-op on a newer nginx and the fix on an older one.
    location ~ \.wasm$ {
        default_type application/wasm;
    }

    gzip on;
    gzip_vary on;
    gzip_min_length 1024;
    # The default gzip_types does not include wasm or the SPA's own
    # types. The two WebAssembly binaries are ~9 MB uncompressed and
    # about a third of that compressed, so this is most of the download.
    gzip_types application/wasm application/javascript text/css
               application/json image/svg+xml application/manifest+json;

    # Content-hashed filenames: a change makes a new name, so these can
    # be cached forever.
    location ~* ^/(app/immutable|fonts)/ {
        add_header Cache-Control "public, max-age=31536000, immutable";
    }

    # These three keep their names across builds, so they must always be
    # revalidated — the service worker in particular. Cache it and a
    # released update can never reach anyone.
    location ~* ^/(index\.html|service-worker\.js|manifest\.webmanifest)$ {
        add_header Cache-Control "no-cache";
    }

    # Single-page app: an unknown path is the app's own route, not a
    # missing file. Without this a reload on /login 404s, which breaks
    # sign-in on the redirect back from Google.
    location / {
        try_files $uri $uri/ /index.html;
    }
}

# Sign-in. Same proxy settings as the accounts block, for the same
# reasons — X-Forwarded-For is what makes
# OPENAPPS_SERVER_TRUST_PROXY_HEADERS meaningful, and without it every
# request looks like it came from the proxy, so one caller can exhaust
# everyone's rate limit.
server {
    listen 80;
    server_name auth.openpdfedit.com;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host              $host;
        proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        # Longer than the server's own 30s timeout, so it decides.
        proxy_read_timeout 60s;
    }
}
NGINX
```

The closing `NGINX` must be alone on its own line — that is what ends
the paste. Check it landed:

```sh
cat /etc/nginx/sites-available/openpdfedit
```

The document roots have to exist before nginx will start cleanly, even
empty:

```sh
mkdir -p /var/www/openpdfedit /var/www/openpdfedit-app
```

Enable it, and check the syntax *before* reloading — a bad config takes
down every site on the box, including the ones already working:

```sh
ln -s /etc/nginx/sites-available/openpdfedit /etc/nginx/sites-enabled/
nginx -t && systemctl reload nginx
```

Then the certificates. `--nginx` edits the blocks above in place, adding
the TLS listeners and the HTTP→HTTPS redirect:

```sh
certbot --nginx -d openpdfedit.com -d www.openpdfedit.com \
                -d app.openpdfedit.com -d auth.openpdfedit.com
```

All four names must already resolve here (§1) — certbot proves control
by answering a challenge on each one, so a name without a DNS record
fails the whole run. Certbot rewrites the four blocks above to listen on
443 and adds an HTTP→HTTPS redirect to each; the `return 301` in the www
block keeps working, since it redirects by name rather than by scheme. Existing certificates are untouched; this adds a
separate one. Renewal is already scheduled by the certbot package:

```sh
systemctl list-timers | grep certbot
```

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

Same `/var/www/<name>` layout as `/var/www/opencapture`. `--delete`
matters on the app: its JavaScript filenames are content hashes, so
without it every deploy leaves the previous build's chunks behind
forever.

If nginx runs as `www-data` and rsync lands files as root, fix the
ownership once after the first deploy:

```sh
chown -R www-data:www-data /var/www/openpdfedit /var/www/openpdfedit-app
```

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
| the app loads but the editor never appears | §2 — `.wasm` served as `application/octet-stream`; check `curl -sI https://app.openpdfedit.com/pdfium.wasm` |
| unlock fails with a network error, nothing in the gateway log | §4 — origin missing from `GATEWAY_ALLOWED_ORIGINS`; the browser blocks it before the request is sent |
| unlock returns 500 | §4 — no `OPENAPPS_KEY_OPENPDFEDIT` in the gateway's environment |
| the app loads yesterday's build | a stale service worker; hard-reload once, then check §5's `--delete` actually ran |
| certbot fails on one name | §1 — that name has no DNS record yet; certbot must answer a challenge on every `-d` |
| `nginx -t` fails after editing | a typo in §2's block — nothing is reloaded until it passes, so the running sites are unaffected |
