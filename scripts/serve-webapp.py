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


class Handler(http.server.SimpleHTTPRequestHandler):
    def guess_type(self, path):
        for suffix, mime in TYPES.items():
            if path.endswith(suffix):
                return mime
        return super().guess_type(path)

    def send_head(self):
        # SPA fallback: a path with no extension is a route, not a file.
        path = self.translate_path(self.path)
        if not os.path.exists(path) and "." not in os.path.basename(path):
            self.path = "/index.html"
        return super().send_head()

    def log_message(self, *args):
        pass


def main() -> None:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8099
    handler = functools.partial(Handler, directory=os.path.abspath(ROOT))
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as httpd:
        print(f"serving {os.path.abspath(ROOT)} on http://127.0.0.1:{port}", flush=True)
        httpd.serve_forever()


if __name__ == "__main__":
    main()
