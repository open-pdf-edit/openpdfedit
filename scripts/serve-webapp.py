#!/usr/bin/env python3
"""Serves apps/webapp/dist for the e2e suite and for hand testing.

`python3 -m http.server` is not enough for two reasons, both of which
present as "the page loads and nothing works":

  * It does not know `.wasm`. `WebAssembly.instantiateStreaming` refuses
    anything that is not `application/wasm`, so the editor never starts
    and the topbar renders zero buttons.
  * There is no SPA fallback, so any route below `/` is a 404.

Port 8099 is the suite's default and is not ours alone -- another app in
this suite binds 127.0.0.1:8099 on the same machine, and Chromium
resolves "localhost" to IPv4, so the tests silently drove a different
product. Pass a port to avoid it:

    python3 scripts/serve-webapp.py 8177
"""
import functools
import http.server
import os
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "apps", "webapp", "dist")

TYPES = {
    ".wasm": "application/wasm",
    ".js": "text/javascript",
    ".mjs": "text/javascript",
    ".json": "application/json",
    ".traineddata": "application/octet-stream",
    ".webmanifest": "application/manifest+json",
}


def detect_base() -> str:
    """The base path this build was compiled for, read out of index.html.

    SvelteKit bakes `BASE_PATH` into every chunk URL *and* into the client
    router, so it cannot be applied or removed by the server afterwards: a
    build made for /app renders "Not found: /" when opened at the root,
    and a base-less build 404s under /app. The web app is currently built
    with BASE_PATH=/app, the desktop app and extension with none — so this
    is detected rather than configured, and the suite keeps working
    whichever way that setting goes next.
    """
    index = os.path.join(ROOT, "index.html")
    try:
        with open(index, encoding="utf-8") as fh:
            html = fh.read()
    except OSError:
        return ""
    marker = "/immutable/"
    at = html.find(marker)
    if at < 0:
        return ""
    start = html.rfind('"', 0, at) + 1
    asset = html[start:at]          # e.g. "/app/app" for base "/app"
    # The build always emits <base>/app/immutable/..., so whatever precedes
    # the final "/app" segment is the base.
    return asset[: -len("/app")] if asset.endswith("/app") else ""


class Handler(http.server.SimpleHTTPRequestHandler):
    def guess_type(self, path):
        for suffix, mime in TYPES.items():
            if path.endswith(suffix):
                return mime
        return super().guess_type(path)

    def send_head(self):
        base = self.server.base_path  # type: ignore[attr-defined]

        # Land a bare visit on the base the build expects, rather than
        # serving index.html at a URL its own router will reject.
        if base and self.path in ("/", ""):
            self.send_response(302)
            self.send_header("Location", base + "/")
            self.end_headers()
            return None

        if base and self.path.startswith(base + "/"):
            self.path = self.path[len(base):] or "/"

        # SPA fallback: a path with no extension is a route, not a file.
        path = self.translate_path(self.path)
        if not os.path.exists(path) and "." not in os.path.basename(path):
            self.path = "/index.html"
        return super().send_head()

    def log_message(self, *args):
        pass


def main() -> None:
    # Fail here, not with a page of 404s. A missing dist/ presents as an
    # app that loads and does nothing, which is a long way from the
    # actual problem.
    if not os.path.isdir(ROOT) or not os.path.exists(os.path.join(ROOT, "index.html")):
        sys.exit(
            f"serve-webapp.py: no build at {os.path.abspath(ROOT)}\n"
            "Run: cd apps/webapp && npm run build"
        )
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8137
    handler = functools.partial(Handler, directory=os.path.abspath(ROOT))
    base = detect_base()
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as httpd:
        httpd.base_path = base  # type: ignore[attr-defined]
        where = f"http://127.0.0.1:{port}{base}/"
        print(f"serving {os.path.abspath(ROOT)} (base {base or '/'}) at {where}", flush=True)
        httpd.serve_forever()


if __name__ == "__main__":
    main()
