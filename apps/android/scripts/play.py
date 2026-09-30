#!/usr/bin/env python3
"""Google Play, from the command line.

The Play Console's own answer to "upload this bundle" is a file picker,
which is fine once and tedious forever — and impossible from a script,
since a browser will not let anything but a person fill a file input.
The Play Developer API has no such problem, so everything after the
one-time console setup happens here.

What the console still has to do, because the API cannot:

  * create the app and its package name (permanent, and the API has no
    method for it);
  * grant this service account access — Users and permissions → Invite
    new users, with the service account's own email as the address. The
    old "API access" page no longer exists;
  * licence testers, under Settings → Licence testing.

Credentials: a service account JSON key, by default at
`~/.config/openpdfedit-android/play-service-account.json`, or wherever
`PLAY_SERVICE_ACCOUNT_JSON` points. It is a private key — keep it out of
the repository, and out of Downloads.

    play.py status                 what Play thinks the app looks like
    play.py upload <file.aab>      upload to the internal testing track
    play.py products               list the in-app products
    play.py create-products        create the credit packs

An "edit" is Play's unit of change: open one, make every change against
it, then commit. Nothing is visible until the commit, and an edit left
uncommitted expires on its own — which makes a failed run harmless
rather than something to clean up.
"""

from __future__ import annotations

import base64
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding

PACKAGE = os.environ.get("PLAY_PACKAGE", "com.openpdfedit.app")
KEY_PATH = os.environ.get(
    "PLAY_SERVICE_ACCOUNT_JSON",
    os.path.expanduser("~/.config/openpdfedit-android/play-service-account.json"),
)
API = "https://androidpublisher.googleapis.com/androidpublisher/v3"
SCOPE = "https://www.googleapis.com/auth/androidpublisher"

# The credit packs. The same ids as iOS and as `app_iap_products` on the
# server, because the server looks a purchase up by product id and does
# not care which store it came from.
PRODUCTS = [
    {
        "sku": "credits_1000",
        "title": "1,000 Credits",
        "description": "1,000 credits to unlock supporter features.",
    },
    {
        "sku": "credits_5000",
        "title": "5,000 Credits",
        "description": "5,000 credits to unlock supporter features.",
    },
]


def die(message: str) -> None:
    print(f"play.py: {message}", file=sys.stderr)
    raise SystemExit(1)


def credentials() -> dict:
    if not os.path.exists(KEY_PATH):
        die(
            f"no service account key at {KEY_PATH}\n"
            "  Google Cloud console → IAM & Admin → Service Accounts → Keys →\n"
            "  Add key → JSON, then move it there and chmod 600."
        )
    with open(KEY_PATH) as f:
        return json.load(f)


def _b64(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode()


_token_cache: tuple[str, float] | None = None


def token() -> str:
    """An OAuth access token, from a self-signed JWT.

    The two-legged flow, so there is nobody to redirect and no refresh
    token to store: the key signs an assertion saying who it is and what
    it wants, and Google answers with an hour's access. Cached for the
    process, because every command below makes several calls.
    """
    global _token_cache
    if _token_cache and _token_cache[1] > time.time() + 60:
        return _token_cache[0]

    creds = credentials()
    now = int(time.time())
    header = {"alg": "RS256", "typ": "JWT", "kid": creds["private_key_id"]}
    claims = {
        "iss": creds["client_email"],
        "scope": SCOPE,
        "aud": creds["token_uri"],
        "iat": now,
        "exp": now + 3600,
    }
    signing_input = f"{_b64(json.dumps(header).encode())}.{_b64(json.dumps(claims).encode())}"
    key = serialization.load_pem_private_key(creds["private_key"].encode(), password=None)
    signature = key.sign(signing_input.encode(), padding.PKCS1v15(), hashes.SHA256())
    assertion = f"{signing_input}.{_b64(signature)}"

    body = urllib.parse.urlencode(
        {"grant_type": "urn:ietf:params:oauth:grant-type:jwt-bearer", "assertion": assertion}
    ).encode()
    request = urllib.request.Request(creds["token_uri"], data=body, method="POST")
    request.add_header("Content-Type", "application/x-www-form-urlencoded")
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as e:
        die(f"could not get a token: {e.read().decode(errors='replace')[:400]}")
    _token_cache = (payload["access_token"], now + payload.get("expires_in", 3600))
    return _token_cache[0]


def call(method: str, path: str, body=None, content_type="application/json", raw=False):
    url = path if path.startswith("http") else f"{API}{path}"
    data = body if raw else (json.dumps(body).encode() if body is not None else None)
    request = urllib.request.Request(url, data=data, method=method)
    request.add_header("Authorization", f"Bearer {token()}")
    if data is not None:
        request.add_header("Content-Type", content_type)
    try:
        with urllib.request.urlopen(request, timeout=600) as response:
            payload = response.read()
            return json.loads(payload) if payload else {}
    except urllib.error.HTTPError as e:
        detail = e.read().decode(errors="replace")
        try:
            message = json.loads(detail)["error"]["message"]
        except Exception:
            message = detail[:500]
        die(f"{method} {path} → HTTP {e.code}\n  {message}")


# --- edits ---------------------------------------------------------------


def open_edit() -> str:
    return call("POST", f"/applications/{PACKAGE}/edits")["id"]


def commit_edit(edit_id: str) -> None:
    call("POST", f"/applications/{PACKAGE}/edits/{edit_id}:commit")


# --- commands ------------------------------------------------------------


def status() -> None:
    edit = open_edit()
    print(f"package: {PACKAGE}")
    bundles = call("GET", f"/applications/{PACKAGE}/edits/{edit}/bundles").get("bundles", [])
    print(f"bundles: {len(bundles)}")
    for b in bundles[-5:]:
        print(f"  versionCode {b['versionCode']}  sha1 {b.get('sha1','')[:12]}…")
    tracks = call("GET", f"/applications/{PACKAGE}/edits/{edit}/tracks").get("tracks", [])
    print("tracks:")
    for t in tracks:
        for r in t.get("releases", []):
            codes = ",".join(str(c) for c in r.get("versionCodes", []) or [])
            print(f"  {t['track']:<18} {r.get('status'):<12} versionCodes[{codes}]")
    # Deliberately not committed: reading should change nothing, and an
    # abandoned edit expires by itself.


def upload(path: str, track: str = "internal") -> None:
    if not os.path.exists(path):
        die(f"no such file: {path}")
    size = os.path.getsize(path)
    print(f"uploading {os.path.basename(path)} ({size / 1e6:.1f} MB)")
    edit = open_edit()
    with open(path, "rb") as f:
        blob = f.read()
    result = call(
        "POST",
        f"https://androidpublisher.googleapis.com/upload/androidpublisher/v3"
        f"/applications/{PACKAGE}/edits/{edit}/bundles?uploadType=media",
        body=blob,
        content_type="application/octet-stream",
        raw=True,
    )
    version_code = result["versionCode"]
    print(f"  uploaded as versionCode {version_code}")
    call(
        "PUT",
        f"/applications/{PACKAGE}/edits/{edit}/tracks/{track}",
        {
            "track": track,
            "releases": [{"versionCodes": [str(version_code)], "status": "draft"}],
        },
    )
    commit_edit(edit)
    # Draft rather than completed on purpose: rolling out to testers is a
    # decision, and one this script should not make on its own.
    print(f"  assigned to '{track}' as a draft release — roll out in the console")


def products() -> None:
    result = call("GET", f"/applications/{PACKAGE}/inappproducts")
    items = result.get("inappproduct", [])
    if not items:
        print("no in-app products")
        return
    for p in items:
        price = p.get("defaultPrice", {})
        print(
            f"{p['sku']:<16} {p.get('status','?'):<10} "
            f"{price.get('priceMicros','?')} {price.get('currency','')}"
        )


def create_products() -> None:
    """Creates the credit packs, without a price.

    Prices are deliberately left unset. Play wants a price per country,
    the tiers do not line up with Apple's, and picking one is a
    commercial decision rather than a technical one — so the products are
    created inactive and priced in the console.
    """
    existing = {
        p["sku"] for p in call("GET", f"/applications/{PACKAGE}/inappproducts").get("inappproduct", [])
    }
    for spec in PRODUCTS:
        if spec["sku"] in existing:
            print(f"{spec['sku']:<16} already exists")
            continue
        call(
            "POST",
            f"/applications/{PACKAGE}/inappproducts?autoConvertMissingPrices=true",
            {
                "packageName": PACKAGE,
                "sku": spec["sku"],
                # Consumable: the server grants credits per purchase and
                # consumes the token afterwards, so the same pack must be
                # buyable again. A managed product that is never consumed
                # can be bought exactly once, ever.
                "purchaseType": "managedUser",
                "status": "inactive",
                "defaultLanguage": "en-US",
                "listings": {
                    "en-US": {"title": spec["title"], "description": spec["description"]}
                },
            },
        )
        print(f"{spec['sku']:<16} created (inactive, unpriced)")


def main() -> None:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        raise SystemExit(2)
    command, rest = args[0], args[1:]
    if command == "status":
        status()
    elif command == "upload":
        if not rest:
            die("upload needs a path to an .aab")
        upload(rest[0], rest[1] if len(rest) > 1 else "internal")
    elif command == "products":
        products()
    elif command == "create-products":
        create_products()
    else:
        print(__doc__)
        raise SystemExit(2)


if __name__ == "__main__":
    main()
