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
    clear_invalid_profiles(PROFILE_NAME)
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


def clear_invalid_profiles(name: str) -> None:
    """Deletes profiles of this name that Apple has marked INVALID.

    Changing an App ID's capabilities — adding Sign in with Apple, say —
    invalidates every profile built on it. An INVALID profile still holds
    its name, and still sits in the listing looking like the one to use,
    so the next archive fails with a signing error that does not mention
    either fact. Replaced here instead.
    """
    for profile in call("GET", "/profiles?limit=200").get("data", []):
        a = profile["attributes"]
        if a["name"] == name and a["profileState"] == "INVALID":
            call("DELETE", f"/profiles/{profile['id']}")
            print(f"deleted invalidated profile {name}")


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




# --- the listing ----------------------------------------------------------------

def listing_copy(platform: str = "IOS") -> dict:
    """The listing, read out of store/listing.md so there is one copy of it.

    The file is written for a person to read — wrapped prose in blockquotes,
    a fenced keyword line, a table of URLs. Everything below unwraps exactly
    that, so editing the listing means editing the document, not this script.
    """
    src = (Path(__file__).resolve().parent.parent / "store" / "listing.md").read_text()

    def blockquote(heading: str) -> str:
        body = src.split(f"## {heading}", 1)[1].split("\n## ", 1)[0]
        quoted, started = [], False
        for line in body.splitlines():
            if line.startswith(">"):
                started = True
                quoted.append("" if line.strip() == ">" else line[1:].lstrip())
            elif started and not line.strip():
                continue
            elif started:
                break
        paragraphs, current = [], []
        for line in quoted:
            bullet = line.startswith(("•", "1.", "2.", "3.", "4."))
            if not line or bullet:
                if current:
                    paragraphs.append((" ".join(current), False))
                    current = []
                if bullet:
                    paragraphs.append((line, True))
            else:
                current.append(line)
        if current:
            paragraphs.append((" ".join(current), False))
        # Bullets sit on consecutive lines; prose paragraphs get a blank line.
        out = ""
        for i, (text, bullet) in enumerate(paragraphs):
            if i:
                out += "\n" if bullet and paragraphs[i - 1][1] else "\n\n"
            out += text
        return out

    mac = platform == "MAC_OS"
    return {
        "promotionalText": blockquote("Mac promotional text (170 max)" if mac else "Promotional text (170 max)"),
        "description": blockquote("Mac description (4000 max)" if mac else "Description (4000 max)"),
        "keywords": re.search(r"## Keywords.*?```\n(.*?)\n```", src, re.S).group(1).strip(),
        "supportUrl": "https://openpdfedit.com/support",
        "marketingUrl": "https://openpdfedit.com",
        "privacyPolicyUrl": "https://openpdfedit.com/privacy",
        "subtitle": "Edit PDFs on device",
    }


def app_id() -> str:
    found = call("GET", f"/apps?filter[bundleId]={BUNDLE_ID}").get("data", [])
    if not found:
        die(f"no app record for {BUNDLE_ID} — App Store Connect → Apps → + is the one step with no API")
    return found[0]["id"]


def version_for(app: str, platform: str) -> dict:
    for v in call("GET", f"/apps/{app}/appStoreVersions?limit=20").get("data", []):
        if v["attributes"]["platform"] == platform:
            return v
    die(f"no {platform} version record on the app")


def ios_version(app: str) -> dict:
    return version_for(app, "IOS")


def marketing_version(platform: str) -> str:
    """The version the binary carries, which the record must match."""
    root = Path(__file__).resolve().parent.parent
    if platform == "MAC_OS":
        conf = json.loads((root.parent / "desktop" / "src-tauri" / "tauri.conf.json").read_text())
        return conf["version"]
    project = (root / "OpenPdfEdit.xcodeproj" / "project.pbxproj").read_text()
    return re.search(r"MARKETING_VERSION = ([^;]+);", project).group(1).strip()


def listing(platform: str = "IOS") -> None:
    copy, app = listing_copy(platform), app_id()
    for field, limit in (("promotionalText", 170), ("description", 4000), ("keywords", 100), ("subtitle", 30)):
        if len(copy[field]) > limit:
            die(f"{field} is {len(copy[field])} characters, over Apple's {limit}")

    version = version_for(app, platform)
    # The build's CFBundleShortVersionString decides which version record it
    # can attach to, so the record follows the binary, not the other way —
    # read from where scripts/set-version.sh writes it.
    marketing = marketing_version(platform)
    if version["attributes"]["versionString"] != marketing:
        call("PATCH", f"/appStoreVersions/{version['id']}", {
            "data": {"type": "appStoreVersions", "id": version["id"],
                     "attributes": {"versionString": marketing, "copyright": "2026 DE JIAN KOH"}},
        })
        print(f"version record → {marketing}")

    localizations = call("GET", f"/appStoreVersions/{version['id']}/appStoreVersionLocalizations").get("data", [])
    body = {k: copy[k] for k in ("description", "keywords", "promotionalText", "supportUrl", "marketingUrl")}
    existing = next((l for l in localizations if l["attributes"]["locale"] == "en-US"), None)
    if existing:
        call("PATCH", f"/appStoreVersionLocalizations/{existing['id']}",
             {"data": {"type": "appStoreVersionLocalizations", "id": existing["id"], "attributes": body}})
    else:
        call("POST", "/appStoreVersionLocalizations", {
            "data": {"type": "appStoreVersionLocalizations", "attributes": {"locale": "en-US", **body},
                     "relationships": {"appStoreVersion": {"data": {"type": "appStoreVersions", "id": version["id"]}}}},
        })
    print(f"description ({len(copy['description'])} chars), keywords, promo text, both URLs")

    info = call("GET", f"/apps/{app}/appInfos").get("data", [])[0]
    for loc in call("GET", f"/appInfos/{info['id']}/appInfoLocalizations").get("data", []):
        if loc["attributes"]["locale"] == "en-US":
            call("PATCH", f"/appInfoLocalizations/{loc['id']}", {
                "data": {"type": "appInfoLocalizations", "id": loc["id"],
                         "attributes": {"subtitle": copy["subtitle"], "privacyPolicyUrl": copy["privacyPolicyUrl"]}},
            })
            print(f"subtitle {copy['subtitle']!r}, privacy policy url")
    call("PATCH", f"/appInfos/{info['id']}", {
        "data": {"type": "appInfos", "id": info["id"], "relationships": {
            "primaryCategory": {"data": {"type": "appCategories", "id": "PRODUCTIVITY"}},
            "secondaryCategory": {"data": {"type": "appCategories", "id": "BUSINESS"}},
        }},
    })
    print("categories: Productivity, Business")

    # 4+: nothing in the app generates or shows anything to rate.
    declaration = call("GET", f"/appInfos/{info['id']}/ageRatingDeclaration").get("data")
    if declaration:
        call("PATCH", f"/ageRatingDeclarations/{declaration['id']}", {
            "data": {"type": "ageRatingDeclarations", "id": declaration["id"], "attributes": {
                "violenceCartoonOrFantasy": "NONE", "violenceRealistic": "NONE",
                "violenceRealisticProlongedGraphicOrSadistic": "NONE",
                "profanityOrCrudeHumor": "NONE", "matureOrSuggestiveThemes": "NONE",
                "horrorOrFearThemes": "NONE", "medicalOrTreatmentInformation": "NONE",
                "alcoholTobaccoOrDrugUseOrReferences": "NONE", "sexualContentOrNudity": "NONE",
                "sexualContentGraphicAndNudity": "NONE", "gamblingSimulated": "NONE",
                "gambling": False, "unrestrictedWebAccess": False,
                "kidsAgeBand": None, "contests": "NONE",
                # Apple added these and rejects the whole declaration when
                # any is absent, naming them one refusal at a time.
                "userGeneratedContent": False, "advertising": False,
                "gunsOrOtherWeapons": "NONE", "healthOrWellnessTopics": False,
                "ageAssurance": False, "lootBox": False,
                "parentalControls": False, "messagingAndChat": False,
            }},
        })
        print("age rating: 4+")

    # This platform's builds only. A universal app's iOS and Mac builds share
    # one list, and "the newest valid one" would otherwise attach a Mac build
    # to the iOS version the moment one existed.
    builds = [b for b in call("GET", f"/builds?filter[app]={app}&filter[preReleaseVersion.platform]={platform}&limit=20").get("data", [])
              if b["attributes"].get("processingState") == "VALID"]
    if builds:
        newest = sorted(builds, key=lambda b: b["attributes"]["uploadedDate"])[-1]
        attached = call("GET", f"/appStoreVersions/{version['id']}/build").get("data") or {}
        # Apple answers a re-attach of the build already there with a 409
        # that reads like a real failure, so it is not asked for.
        if attached.get("id") == newest["id"]:
            print(f"build {newest['attributes']['version']} already attached")
        else:
            call("PATCH", f"/appStoreVersions/{version['id']}/relationships/build",
                 {"data": {"type": "builds", "id": newest["id"]}})
            print(f"build {newest['attributes']['version']} attached")
    else:
        print("no processed build to attach yet — Apple is still processing the upload")



def screenshots(platform: str = "IOS") -> None:
    """Upload store/screenshots to the version, one set per device size.

    Apple takes a screenshot in three steps — reserve, PUT the bytes to the
    url it hands back, then confirm with the file's MD5 — and a screenshot
    that stops after the PUT sits in the set forever as "processing failed".
    """
    import hashlib

    folder = Path(__file__).resolve().parent.parent / "store" / "screenshots"
    # 6.9-inch iPhone and 13-inch iPad: the two sizes a universal app must
    # give, and the only two these captures are made at.
    # By name: NN-name.png is iPhone, NN-name-ipad.png iPad, NN-name-mac.png
    # the Mac. Only the numbered ones — iap-review.png is the in-app
    # purchase's review image, not a listing screenshot.
    numbered = sorted(f for f in folder.glob("[0-9][0-9]-*.png"))
    if platform == "MAC_OS":
        sets = {"APP_DESKTOP": [f for f in numbered if f.stem.endswith("-mac")]}
    else:
        sets = {"APP_IPHONE_67": [f for f in numbered if not f.stem.endswith(("-ipad", "-mac"))],
                "APP_IPAD_PRO_3GEN_129": [f for f in numbered if f.stem.endswith("-ipad")]}

    version = version_for(app_id(), platform)
    localization = next(l for l in call("GET", f"/appStoreVersions/{version['id']}/appStoreVersionLocalizations")["data"]
                        if l["attributes"]["locale"] == "en-US")
    existing = {s["attributes"]["screenshotDisplayType"]: s["id"]
                for s in call("GET", f"/appStoreVersionLocalizations/{localization['id']}/appScreenshotSets").get("data", [])}

    for display_type, files in sets.items():
        if not files:
            print(f"{display_type}: no files"); continue
        set_id = existing.get(display_type)
        if not set_id:
            set_id = call("POST", "/appScreenshotSets", {
                "data": {"type": "appScreenshotSets", "attributes": {"screenshotDisplayType": display_type},
                         "relationships": {"appStoreVersionLocalization": {"data": {"type": "appStoreVersionLocalizations", "id": localization["id"]}}}},
            })["data"]["id"]
        already = {s["attributes"]["fileName"] for s in call("GET", f"/appScreenshotSets/{set_id}/appScreenshots").get("data", [])}

        for file in files:
            if file.name in already:
                print(f"  {file.name} already there"); continue
            content = file.read_bytes()
            reserved = call("POST", "/appScreenshots", {
                "data": {"type": "appScreenshots", "attributes": {"fileSize": len(content), "fileName": file.name},
                         "relationships": {"appScreenshotSet": {"data": {"type": "appScreenshotSets", "id": set_id}}}},
            })["data"]
            for operation in reserved["attributes"]["uploadOperations"]:
                request = urllib.request.Request(operation["url"], data=content, method=operation["method"])
                for header in operation.get("requestHeaders", []):
                    request.add_header(header["name"], header["value"])
                with urllib.request.urlopen(request, timeout=300) as response:
                    if response.status >= 300:
                        die(f"upload of {file.name} returned {response.status}")
            call("PATCH", f"/appScreenshots/{reserved['id']}", {
                "data": {"type": "appScreenshots", "id": reserved["id"],
                         "attributes": {"uploaded": True, "sourceFileChecksum": hashlib.md5(content).hexdigest()}},
            })
            print(f"  {file.name} uploaded ({len(content) // 1024} KB)")
        print(f"{display_type}: {len(files)} screenshot(s)")


# The two credit packs, exactly as store/listing.md and Store.swift name
# them. A mismatch here is a purchase that takes the money and grants
# nothing, and nothing in the toolchain checks it for you.
IAPS = [
    {"productId": "credits_1000", "name": "1,000 Credits", "price": "4.99",
     "description": "1,000 credits for OpenPdfEdit — exactly what the one-time Watermark and OCR unlock costs. Credits never expire."},
    {"productId": "credits_5000", "name": "5,000 Credits", "price": "19.99",
     "description": "5,000 credits for OpenPdfEdit. Credits never expire and can be spent in any OpenApps product."},
]


def iaps() -> None:
    """Create the consumables, name them, and put them on the US price."""
    app = app_id()
    have = {p["attributes"]["productId"]: p for p in call("GET", f"/apps/{app}/inAppPurchasesV2?limit=50").get("data", [])}

    for wanted in IAPS:
        product = have.get(wanted["productId"])
        if product:
            print(f"{wanted['productId']} exists ({product['attributes'].get('state')})")
        else:
            product = call("POST", "/inAppPurchases", {
                "data": {"type": "inAppPurchases", "attributes": {
                    "name": wanted["name"], "productId": wanted["productId"],
                    "inAppPurchaseType": "CONSUMABLE",
                    "reviewNote": "Credits are spent inside the app on the one-time Watermark and OCR unlock. "
                                  "Sign in with the demo account in App Review Information, tap the Watermark tool, "
                                  "and the credits panel appears.",
                }, "relationships": {"app": {"data": {"type": "apps", "id": app}}}},
            })["data"]
            print(f"{wanted['productId']} created")

        localizations = call("GET", f"/inAppPurchases/{product['id']}/inAppPurchaseLocalizations").get("data", [])
        if not any(l["attributes"]["locale"] == "en-US" for l in localizations):
            call("POST", "/inAppPurchaseLocalizations", {
                "data": {"type": "inAppPurchaseLocalizations", "attributes": {
                    "locale": "en-US", "name": wanted["name"], "description": wanted["description"]},
                    "relationships": {"inAppPurchaseV2": {"data": {"type": "inAppPurchases", "id": product["id"]}}}},
            })
            print(f"  described")

        schedule = call("GET", f"/inAppPurchases/{product['id']}/iapPriceSchedule").get("data")
        if schedule:
            print("  priced already")
            continue
        # A price is chosen by naming Apple's own price point for the US
        # territory; the other territories follow from it.
        points = call("GET", f"/inAppPurchases/{product['id']}/pricePoints?filter[territory]=USA&limit=200").get("data", [])
        match = next((p for p in points if p["attributes"]["customerPrice"] == wanted["price"]), None)
        if not match:
            print(f"  no ${wanted['price']} price point among {len(points)}"); continue
        call("POST", "/inAppPurchasePriceSchedules", {
            "data": {"type": "inAppPurchasePriceSchedules",
                     "relationships": {
                         "inAppPurchase": {"data": {"type": "inAppPurchases", "id": product["id"]}},
                         "baseTerritory": {"data": {"type": "territories", "id": "USA"}},
                         "manualPrices": {"data": [{"type": "inAppPurchasePrices", "id": "${price}"}]},
                     }},
            "included": [{"type": "inAppPurchasePrices", "id": "${price}", "attributes": {"startDate": None},
                          "relationships": {"inAppPurchasePricePoint": {"data": {"type": "inAppPurchasePricePoints", "id": match["id"]}}}}],
        })
        print(f"  priced at ${wanted['price']}")


def content_rights() -> None:
    """The declaration App Store Connect requires before a first submission.

    Everything the app shows is the customer's own document or the app's
    own interface; nothing is licensed from anyone else.
    """
    app = app_id()
    call("PATCH", f"/apps/{app}", {"data": {"type": "apps", "id": app,
         "attributes": {"contentRightsDeclaration": "DOES_NOT_USE_THIRD_PARTY_CONTENT"}}})
    print("content rights: does not use third-party content")


def availability() -> None:
    """On sale in every territory, and in any Apple adds later.

    Without this the app can be approved and still be sold nowhere. It is
    a v2 resource, created once; a 409 means it already exists.
    """
    app = app_id()
    territories = []
    url = "/territories?limit=200"
    while url:
        page = call("GET", url)
        territories += [t["id"] for t in page.get("data", [])]
        url = page.get("links", {}).get("next")
    # A local id per territory, "${USA}" — one braces pair is the local-id
    # syntax, the other is the f-string's own escaping.
    included = [{"type": "territoryAvailabilities", "id": f"${{{t}}}",
                 "attributes": {"available": True},
                 "relationships": {"territory": {"data": {"type": "territories", "id": t}}}}
                for t in territories]
    call("POST", "https://api.appstoreconnect.apple.com/v2/appAvailabilities", {
        "data": {"type": "appAvailabilities", "attributes": {"availableInNewTerritories": True},
                 "relationships": {
                     "app": {"data": {"type": "apps", "id": app}},
                     "territoryAvailabilities": {"data": [{"type": "territoryAvailabilities", "id": i["id"]} for i in included]},
                 }},
        "included": included,
    })
    print(f"availability: {len(territories)} territories, and new ones as Apple adds them")


# --- the Mac App Store ------------------------------------------------------------
#
# The same universal bundle id, so one purchase and one set of credit packs
# cover iPhone, iPad and Mac. Two things differ from iOS: the package is a
# .pkg, which needs an installer certificate of its own, and the profile is
# embedded in the .app rather than chosen by Xcode at archive time.

MAC_PROFILE_NAME = os.environ.get("MAC_PROFILE_NAME", f"{APP_NAME} Mac App Store")
MAC_PROFILE_OUT = Path(__file__).resolve().parents[2] / "desktop" / "src-tauri" / "appstore" / "embedded.provisionprofile"


def installed_identity(prefix: str) -> str | None:
    listing = run("security", "find-identity", "-v")
    match = re.search(r'"(' + re.escape(prefix) + r'[^"]+)"', listing)
    return match.group(1) if match else None


def ensure_installer_cert() -> None:
    """The Mac Installer Distribution certificate that signs the .pkg.

    Same shape as ensure_cert: the key is generated here, Apple signs only
    the request. Its identity reads "3rd Party Mac Developer Installer".
    """
    existing = installed_identity("3rd Party Mac Developer Installer") or installed_identity("Mac Installer Distribution")
    if existing:
        print(f"already have {existing}")
        return
    WORK.mkdir(parents=True, exist_ok=True)
    key_path, csr_path, cer_path = WORK / "installer.key", WORK / "installer.csr", WORK / "installer.cer"
    if not key_path.exists():
        run("openssl", "genrsa", "-out", str(key_path), "2048")
        key_path.chmod(0o600)
    run("openssl", "req", "-new", "-key", str(key_path), "-out", str(csr_path),
        "-subj", f"/CN={APP_NAME} Installer/O={APP_NAME}/C=US")
    created = call("POST", "/certificates", {
        "data": {"type": "certificates", "attributes": {
            "certificateType": "MAC_INSTALLER_DISTRIBUTION", "csrContent": csr_path.read_text()}},
    })
    cer_path.write_bytes(base64.b64decode(created["data"]["attributes"]["certificateContent"]))
    pem = WORK / "installer.pem"
    pem.write_text(run("openssl", "x509", "-inform", "DER", "-in", str(cer_path)))
    p12, password = WORK / "installer.p12", base64.urlsafe_b64encode(os.urandom(18)).decode()
    run("openssl", "pkcs12", "-export", "-legacy", "-out", str(p12), "-inkey", str(key_path),
        "-in", str(pem), "-passout", f"pass:{password}")
    p12.chmod(0o600)
    run("security", "import", str(p12), "-k", str(Path.home() / "Library/Keychains/login.keychain-db"),
        "-P", password, "-T", "/usr/bin/productbuild", "-T", "/usr/bin/security")
    p12.unlink()
    print("installed", installed_identity("3rd Party Mac Developer Installer") or installed_identity("Mac Installer Distribution") or "(no identity appeared — check the WWDR intermediate)")


def ensure_mac_profile() -> None:
    """The MAC_APP_STORE profile, written where the store build embeds it."""
    clear_invalid_profiles(MAC_PROFILE_NAME)
    bundle_id = ensure_app_id()
    certs = [c for c in call("GET", "/certificates?limit=200").get("data", [])
             if c["attributes"]["certificateType"] == "DISTRIBUTION"]
    if not certs:
        die("no Apple Distribution certificate — run ensure-cert first")
    profile = next((p for p in call("GET", "/profiles?limit=200").get("data", [])
                    if p["attributes"]["name"] == MAC_PROFILE_NAME and p["attributes"]["profileState"] == "ACTIVE"), None)
    if not profile:
        profile = call("POST", "/profiles", {
            "data": {"type": "profiles", "attributes": {"name": MAC_PROFILE_NAME, "profileType": "MAC_APP_STORE"},
                     "relationships": {
                         "bundleId": {"data": {"type": "bundleIds", "id": bundle_id}},
                         "certificates": {"data": [{"type": "certificates", "id": c["id"]} for c in certs]},
                     }},
        })["data"]
        print(f"created profile {MAC_PROFILE_NAME}")
    content = base64.b64decode(profile["attributes"]["profileContent"])
    MAC_PROFILE_OUT.parent.mkdir(parents=True, exist_ok=True)
    MAC_PROFILE_OUT.write_bytes(content)
    plist = plistlib.loads(re.search(rb"<\?xml.*</plist>", content, re.S).group(0))
    print(f"wrote {MAC_PROFILE_OUT} ({plist['Name']}, expires {plist['ExpirationDate']:%Y-%m-%d})")


def review_notes() -> None:
    """"Notes for the reviewer" on both versions, from store/listing.md.

    The sign-in paragraph first — which field the key is in has no answer
    anywhere else on the form — then the notes that pre-empt the two
    classic rejections: 4.2 (this is a web wrapper) and 3.1.1 (credits
    sold outside in-app purchase). Contact details and the demo account
    fields are left exactly as they are.
    """
    src = (Path(__file__).resolve().parent.parent / "store" / "listing.md").read_text()

    def quoted(heading: str) -> str:
        body = src.split(f"## {heading}\n", 1)[1].split("\n## ", 1)[0]
        lines = [l[1:].lstrip() if l.strip() != ">" else "" for l in body.splitlines() if l.startswith(">")]
        paragraphs, current = [], []
        for line in lines:
            if not line or line[:2] in ("1.", "2.", "3.", "4.") or line.isupper():
                if current:
                    paragraphs.append(" ".join(current)); current = []
                if line:
                    current.append(line)
            else:
                current.append(line)
        if current:
            paragraphs.append(" ".join(current))
        return "\n\n".join(paragraphs)

    signin = quoted("Review notes: sign-in (both platforms)")
    # Apple asked for these on the iOS 2.1 reply and said to keep them for
    # future submissions, so the Mac gets them too rather than waiting to
    # be asked the same six questions.
    about = quoted("Review notes: about the app (both platforms)")
    app = app_id()
    for platform, heading in (("IOS", "Review notes"), ("MAC_OS", "Mac review notes")):
        notes = signin + "\n\n" + quoted(heading) + "\n\n" + about
        if len(notes) > 4000:
            die(f"{platform} notes are {len(notes)} characters, over Apple's 4000")
        version = version_for(app, platform)
        detail = call("GET", f"/appStoreVersions/{version['id']}/appStoreReviewDetail").get("data")
        if not detail:
            die(f"{platform} has no App Review Information yet — its contact details come from the form")
        call("PATCH", f"/appStoreReviewDetails/{detail['id']}",
             {"data": {"type": "appStoreReviewDetails", "id": detail["id"], "attributes": {"notes": notes}}})
        print(f"{platform}: notes set ({len(notes)} characters)")


def main() -> None:
    commands = {
        "whoami": whoami,
        "ensure-cert": ensure_cert,
        "ensure-app-id": lambda: print(f"app id record {ensure_app_id()}"),
        "ensure-profile": ensure_profile,
        "setup": lambda: (ensure_cert(), ensure_app_id(), ensure_profile()),
        "listing": listing,
        "review-notes": review_notes,
        "mac-listing": lambda: listing("MAC_OS"),
        "screenshots": screenshots,
        "mac-screenshots": lambda: screenshots("MAC_OS"),
        "iaps": iaps,
        "content-rights": content_rights,
        "availability": availability,
        "ensure-installer-cert": ensure_installer_cert,
        "ensure-mac-profile": ensure_mac_profile,
    }
    if len(sys.argv) != 2 or sys.argv[1] not in commands:
        print(__doc__)
        raise SystemExit(2)
    commands[sys.argv[1]]()


if __name__ == "__main__":
    main()
