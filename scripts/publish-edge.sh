#!/usr/bin/env bash
# Push a new extension package to Microsoft Edge Add-ons.
#
#   ./scripts/publish-edge.sh                    # upload the current zip and publish it
#   ./scripts/publish-edge.sh --no-publish       # upload to the draft only, publish by hand
#   ./scripts/publish-edge.sh --notes "…"        # certification notes for the reviewer
#   ./scripts/publish-edge.sh path/to/other.zip  # a package other than the default
#
# ## This cannot make the first submission
#
# Edge's Update REST API has endpoints for replacing the *package* of a
# product that already exists, and nothing else — no endpoint creates a
# product, and none edits the listing text, screenshots, or category.
# Microsoft is explicit about this: "To initially publish a new
# extension, you use Partner Center." So the first release is a human
# filling in a form, once; this script is for every release after that.
#
# ## Credentials
#
#   EDGE_PRODUCT_ID   the 128-bit GUID Partner Center gives the product
#   EDGE_CLIENT_ID    from Publish API -> Create API credentials
#   EDGE_API_KEY      created at the same moment, shown once
#
# API v1.1 (the current one — v1's access-token flow was retired at the
# end of 2024) authenticates with the key directly, so there is no token
# exchange step and nothing to cache. The key expires; when it does, the
# upload fails with 401 and the fix is to mint a new one in Partner
# Center, not to change anything here.
#
# Keep these in GitHub Actions secrets or your shell, never in the repo.
set -euo pipefail

cd "$(dirname "$0")/.."

API="https://api.addons.microsoftedge.microsoft.com"
ZIP="apps/extension/openpdfedit-dist.zip"
NOTES="Package update from the OpenPdfEdit repository."
PUBLISH=1

while [ $# -gt 0 ]; do
  case "$1" in
    --no-publish) PUBLISH="" ;;
    --notes) shift; NOTES="${1:?--notes needs a value}" ;;
    -h|--help) sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) ZIP="$1" ;;
  esac
  shift
done

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }
die() { printf '\033[31merror: %s\033[0m\n' "$1" >&2; exit 1; }

for var in EDGE_PRODUCT_ID EDGE_CLIENT_ID EDGE_API_KEY; do
  [ -n "${!var:-}" ] || die "$var is not set — see this script's header"
done

[ -f "$ZIP" ] || die "no package at $ZIP — run (cd apps/extension && npm run package)"

# The zip is a build artifact and goes stale silently: it is not in git,
# so nothing about a normal checkout or pull refreshes it, and uploading
# a months-old build looks exactly like uploading a current one. Refuse
# rather than publish the wrong bytes.
zip_epoch="$(date -r "$ZIP" +%s 2>/dev/null || stat -c %Y "$ZIP")"
head_epoch="$(git log -1 --format=%ct)"
if [ "$zip_epoch" -lt "$head_epoch" ]; then
  behind="$(git log --oneline --since="@$zip_epoch" | wc -l | tr -d ' ')"
  die "$ZIP was built before the current commit ($behind commits ago).
       Rebuild it:  (cd apps/extension && npm run package)"
fi

manifest_version="$(unzip -p "$ZIP" manifest.json | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
[ -n "$manifest_version" ] || die "could not read a version out of $ZIP's manifest.json"

auth=(-H "Authorization: ApiKey $EDGE_API_KEY" -H "X-ClientID: $EDGE_CLIENT_ID")

# Both long-running endpoints answer 202 with the operation id in a
# `Location` response header. Pull it off the headers rather than
# splitting the line on ":" the way Microsoft's own PowerShell sample
# does — that sample truncates any value containing a colon, and quietly
# returns an empty id when the header is absent.
operation_id() {
  tr -d '\r' < "$1" \
    | sed -n 's/^[Ll]ocation:[[:space:]]*//p' \
    | tail -1 \
    | sed 's#.*/##'
}

# Poll until the operation stops being InProgress. Edge reports failure
# in the body of a 200, so a non-empty errors array matters as much as
# the HTTP status.
await() {
  local url="$1" label="$2" tries=0 body status
  while [ "$tries" -lt 60 ]; do
    body="$(curl -sS --fail-with-body "${auth[@]}" -X GET "$url")" \
      || die "$label: status request failed — $body"
    status="$(printf '%s' "$body" | sed -n 's/.*"status"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
    case "$status" in
      Succeeded) log "$label: succeeded"; return 0 ;;
      Failed)    die "$label failed: $body" ;;
      "")        die "$label: no status in response — $body" ;;
    esac
    tries=$((tries + 1))
    sleep 5
  done
  die "$label: still InProgress after five minutes — check Partner Center"
}

headers="$(mktemp)"
trap 'rm -f "$headers"' EXIT

log "Uploading $ZIP (manifest version $manifest_version)"
http="$(curl -sS -o /dev/null -D "$headers" -w '%{http_code}' \
  "${auth[@]}" \
  -H "Content-Type: application/zip" \
  -X POST --data-binary "@$ZIP" \
  "$API/v1/products/$EDGE_PRODUCT_ID/submissions/draft/package")"

[ "$http" = "202" ] || die "upload returned $http, expected 202
       401/403 usually means the API key expired — mint a new one in Partner Center.
       404 usually means EDGE_PRODUCT_ID does not name a product on this account."

upload_op="$(operation_id "$headers")"
[ -n "$upload_op" ] || die "upload was accepted but returned no Location header"
await "$API/v1/products/$EDGE_PRODUCT_ID/submissions/draft/package/operations/$upload_op" "Package upload"

if [ -z "$PUBLISH" ]; then
  log "Uploaded to the draft. Review and publish it in Partner Center."
  exit 0
fi

log "Publishing the draft"
http="$(curl -sS -o /dev/null -D "$headers" -w '%{http_code}' \
  "${auth[@]}" \
  -H "Content-Type: application/json" \
  -X POST --data "$(printf '{"notes":"%s"}' "${NOTES//\"/\\\"}")" \
  "$API/v1/products/$EDGE_PRODUCT_ID/submissions")"

[ "$http" = "202" ] || die "publish returned $http, expected 202"

publish_op="$(operation_id "$headers")"
[ -n "$publish_op" ] || die "publish was accepted but returned no Location header"
await "$API/v1/products/$EDGE_PRODUCT_ID/submissions/operations/$publish_op" "Publish"

log "Submitted for certification. Edge review typically takes a few days."
