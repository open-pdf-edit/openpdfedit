#!/usr/bin/env python3
"""App Store Connect, from the command line: the signing assets an archive needs.

Xcode's own answer to "no signing identity" is Settings → Accounts → Manage
Certificates → +, which is a person clicking in a GUI on one particular Mac.
Everything it does there is in the App Store Connect API, and this does it
with the API key already on this machine: an Apple Distribution certificate
whose private key stays here, the App ID, and the App Store provisioning
profile that ties them together.

    export ASC_KEY_ID=XXXXXXXXXX ASC_ISSUER_ID=<uuid-from-the-keys-page>
    ./scripts/asc.py whoami          # does the key work, and for whom
    ./scripts/asc.py ensure-cert     # Apple Distribution cert + key, in the keychain
    ./scripts/asc.py ensure-app-id   # the bundle id, with In-App Purchase
    ./scripts/asc.py ensure-profile  # the App Store profile, installed
    ./scripts/asc.py setup           # all three, in order

The .p8 is read from ~/.appstoreconnect/private_keys/AuthKey_<KEY_ID>.p8,
where altool and notarytool also look. It is never printed and never copied.

What this cannot do: create the app record itself. App Store Connect has no
API for that first step — someone opens the site once, clicks +, and gives
it the bundle id and SKU. Everything after that is here.
"""
from __future__ import annotations

import base64
import json
import os
import plistlib
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, utils as asym_utils

API = "https://api.appstoreconnect.apple.com/v1"
BUNDLE_ID = os.environ.get("IOS_BUNDLE_ID", "com.openpdfedit.app")
APP_NAME = os.environ.get("IOS_APP_NAME", "OpenPdfEdit")
PROFILE_NAME = os.environ.get("IOS_PROFILE_NAME", f"{APP_NAME} App Store")
KEY_DIR = Path.home() / ".appstoreconnect" / "private_keys"
WORK = Path(__file__).resolve().parent.parent / ".build" / "signing"


def die(message: str) -> "None":
    print(f"asc.py: {message}", file=sys.stderr)
    raise SystemExit(1)


def env(name: str) -> str:
    value = os.environ.get(name, "").strip()
    if not value:
        die(f"{name} is not set — Users and Access → Integrations → Keys has both values")
    return value


def token() -> str:
    """A twenty-minute ES256 JWT, the only thing the API accepts."""
    key_id, issuer = env("ASC_KEY_ID"), env("ASC_ISSUER_ID")
    path = KEY_DIR / f"AuthKey_{key_id}.p8"
    if not path.exists():
        die(f"no key at {path} — Apple lets that file be downloaded once, so it has to be the one you kept")
    private = serialization.load_pem_private_key(path.read_bytes(), password=None)

    def part(obj: dict) -> bytes:
        return base64.urlsafe_b64encode(json.dumps(obj, separators=(",", ":")).encode()).rstrip(b"=")

    header = part({"alg": "ES256", "kid": key_id, "typ": "JWT"})
    now = int(time.time())
    payload = part({"iss": issuer, "iat": now, "exp": now + 20 * 60, "aud": "appstoreconnect-v1"})
    signed = private.sign(header + b"." + payload, ec.ECDSA(hashes.SHA256()))
    # JOSE wants the raw r||s pair, not the DER sequence OpenSSL produces.
    r, s = asym_utils.decode_dss_signature(signed)
    raw = r.to_bytes(32, "big") + s.to_bytes(32, "big")
    return (header + b"." + payload + b"." + base64.urlsafe_b64encode(raw).rstrip(b"=")).decode()


def call(method: str, path: str, body: dict | None = None) -> dict:
    url = path if path.startswith("http") else f"{API}{path}"
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(url, data=data, method=method)
    request.add_header("Authorization", f"Bearer {token()}")
    if data:
        request.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            raw = response.read()
            return json.loads(raw) if raw else {}
    except urllib.error.HTTPError as e:
        detail = e.read().decode(errors="replace")
        try:  # Apple's errors say exactly what is wrong; show that, not the status.
            for err in json.loads(detail).get("errors", []):
                print(f"  {err.get('title')}: {err.get('detail')}", file=sys.stderr)
        except Exception:
            print(f"  {detail[:400]}", file=sys.stderr)
        die(f"{method} {path} → HTTP {e.code}")


def run(*args: str, input_bytes: bytes | None = None) -> str:
    done = subprocess.run(args, input=input_bytes, capture_output=True)
    if done.returncode != 0:
        die(f"{' '.join(args[:3])}… failed: {done.stderr.decode(errors='replace').strip()[:300]}")
    return done.stdout.decode(errors="replace")


# --- what the archive needs -----------------------------------------------------

def whoami() -> None:
    apps = call("GET", "/apps?limit=200").get("data", [])
    print(f"key {env('ASC_KEY_ID')} works. {len(apps)} app record(s):")
    for app in apps:
        a = app["attributes"]
        print(f"  {a.get('bundleId'):40} {a.get('name')}  (sku {a.get('sku')})")
    if not any(app["attributes"].get("bundleId") == BUNDLE_ID for app in apps):
        print(f"\n  {BUNDLE_ID} has no app record yet. That one step is the website's alone:")
        print(f"  App Store Connect → Apps → + → iOS, bundle id {BUNDLE_ID}, SKU openpdfedit-ios.")


def installed_distribution_identity() -> str | None:
    listing = run("security", "find-identity", "-v", "-p", "codesigning")
    match = re.search(r'"(Apple Distribution: [^"]+)"', listing)
    return match.group(1) if match else None


def ensure_cert() -> None:
    """An Apple Distribution certificate whose private key is in this keychain.

    The key is generated here and never leaves: Apple only ever sees the
    certificate request, and signs the public half of it.
    """
    existing = installed_distribution_identity()
    if existing:
        print(f"already have {existing}")
        return

    WORK.mkdir(parents=True, exist_ok=True)
    key_path, csr_path, cer_path = WORK / "distribution.key", WORK / "distribution.csr", WORK / "distribution.cer"
    if not key_path.exists():
        run("openssl", "genrsa", "-out", str(key_path), "2048")
        key_path.chmod(0o600)
    run("openssl", "req", "-new", "-key", str(key_path), "-out", str(csr_path),
        "-subj", f"/CN={APP_NAME} Distribution/O={APP_NAME}/C=US")

    print("asking Apple to sign the certificate request")
    created = call("POST", "/certificates", {
        "data": {"type": "certificates", "attributes": {
            "certificateType": "DISTRIBUTION",
            "csrContent": csr_path.read_text(),
        }},
    })
    cer_path.write_bytes(base64.b64decode(created["data"]["attributes"]["certificateContent"]))

    # Into the keychain as one item, so codesign can find the pair.
    pem = WORK / "distribution.pem"
    pem.write_text(run("openssl", "x509", "-inform", "DER", "-in", str(cer_path)))
    p12, password = WORK / "distribution.p12", base64.urlsafe_b64encode(os.urandom(18)).decode()
    run("openssl", "pkcs12", "-export", "-legacy", "-out", str(p12), "-inkey", str(key_path),
        "-in", str(pem), "-passout", f"pass:{password}")
    p12.chmod(0o600)
    run("security", "import", str(p12), "-k", str(Path.home() / "Library/Keychains/login.keychain-db"),
        "-P", password, "-T", "/usr/bin/codesign", "-T", "/usr/bin/security")
    p12.unlink()

    identity = installed_distribution_identity()
    print(f"installed {identity}" if identity else "the certificate imported but no identity appeared — is the WWDR intermediate present?")


def ensure_app_id() -> str:
    """The App ID Apple knows, with the capabilities the app actually uses."""
    found = call("GET", f"/bundleIds?filter[identifier]={BUNDLE_ID}&limit=200").get("data", [])
    if found:
        print(f"app id {BUNDLE_ID} exists")
        return found[0]["id"]
    print(f"registering {BUNDLE_ID}")
    created = call("POST", "/bundleIds", {
        "data": {"type": "bundleIds", "attributes": {
            "identifier": BUNDLE_ID, "name": APP_NAME, "platform": "IOS",
        }},
    })
    bundle_id = created["data"]["id"]
    # In-app purchase is what pays for the two paid tools; without it the
    # profile is valid and StoreKit finds no products at runtime.
    call("POST", "/bundleIdCapabilities", {
        "data": {"type": "bundleIdCapabilities", "attributes": {"capabilityType": "IN_APP_PURCHASE"},
                 "relationships": {"bundleId": {"data": {"type": "bundleIds", "id": bundle_id}}}},
    })
    return bundle_id


def ensure_profile() -> None:
    """The App Store profile, installed where Xcode looks for it."""
    bundle_id = ensure_app_id()
    certs = [c for c in call("GET", "/certificates?limit=200").get("data", [])
             if c["attributes"]["certificateType"] in ("DISTRIBUTION", "IOS_DISTRIBUTION")]
    if not certs:
        die("no distribution certificate on the account — run ensure-cert first")

    for profile in call("GET", "/profiles?limit=200").get("data", []):
        if profile["attributes"]["name"] == PROFILE_NAME and profile["attributes"]["profileState"] == "ACTIVE":
            print(f"profile {PROFILE_NAME} exists")
            install_profile(profile)
            return

    print(f"creating profile {PROFILE_NAME}")
    created = call("POST", "/profiles", {
        "data": {"type": "profiles", "attributes": {
            "name": PROFILE_NAME, "profileType": "IOS_APP_STORE",
        }, "relationships": {
            "bundleId": {"data": {"type": "bundleIds", "id": bundle_id}},
            "certificates": {"data": [{"type": "certificates", "id": c["id"]} for c in certs]},
        }},
    })
    install_profile(created["data"])


def install_profile(profile: dict) -> None:
    content = base64.b64decode(profile["attributes"]["profileContent"])
    # Xcode reads profiles by UUID from this directory; the name is ignored.
    plist = plistlib.loads(re.search(rb"<\?xml.*</plist>", content, re.S).group(0))
    target = Path.home() / "Library/MobileDevice/Provisioning Profiles"
    target.mkdir(parents=True, exist_ok=True)
    path = target / f"{plist['UUID']}.mobileprovision"
    path.write_bytes(content)
    print(f"installed {path.name} ({plist['Name']}, expires {plist['ExpirationDate']:%Y-%m-%d})")
    print(f"  PROVISIONING_PROFILE_SPECIFIER={plist['Name']}")


def main() -> None:
    commands = {
        "whoami": whoami,
        "ensure-cert": ensure_cert,
        "ensure-app-id": lambda: print(f"app id record {ensure_app_id()}"),
        "ensure-profile": ensure_profile,
        "setup": lambda: (ensure_cert(), ensure_app_id(), ensure_profile()),
    }
    if len(sys.argv) != 2 or sys.argv[1] not in commands:
        print(__doc__)
        raise SystemExit(2)
    commands[sys.argv[1]]()


if __name__ == "__main__":
    main()
