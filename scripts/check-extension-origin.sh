#!/usr/bin/env bash
# Asks the production servers whether they will talk to the extension.
#
#   ./scripts/check-extension-origin.sh <extension id>
#
# The extension requests no host permissions — that is deliberate, and it
# is most of what "your documents never leave your machine" means — so
# every call it makes to a server is an ordinary cross-origin request
# governed by CORS. Its origin therefore has to be on two allowlists, and
# if it is not, the extension signs in successfully and then fails on
# every call afterwards with "Could not reach the server. Check your
# connection."
#
# That failure is invisible in development, because the unpacked
# extension has a different id from the published one and the dev id is
# usually the one that got allowlisted. It is what a store reviewer sees
# first. Run this against the *published* id before answering a
# certification report.
#
# Find the id in Partner Center → Microsoft Edge → OpenPdfEdit →
# Extension overview. It is 32 lower-case letters, and Microsoft assigns
# it at first submission — it does not change afterwards, and it is not
# the Product ID (a uuid) that the certification report quotes.
set -euo pipefail

ID="${1:-}"
if [ -z "$ID" ]; then
  echo "usage: $0 <extension id>" >&2
  exit 64
fi
if ! printf '%s' "$ID" | grep -Eq '^[a-p]{32}$'; then
  echo "check-extension-origin.sh: '$ID' is not an extension id." >&2
  echo "  Expected 32 letters a-p. A uuid with dashes is the Partner Center" >&2
  echo "  Product ID, which is a different thing and will not work here." >&2
  exit 64
fi

ORIGIN="chrome-extension://$ID"
FAILED=0

probe() {
  local label="$1" url="$2" allowed
  allowed="$(curl -s -i -X OPTIONS "$url" \
    -H "Origin: $ORIGIN" \
    -H 'Access-Control-Request-Method: GET' \
    -H 'Access-Control-Request-Headers: authorization' \
    --max-time 20 \
    | tr -d '\r' | grep -i '^access-control-allow-origin:' | cut -d' ' -f2- || true)"

  if [ "$allowed" = "$ORIGIN" ]; then
    printf '  \033[32m%-7s\033[0m %-8s %s\n' OK "$label" "$url"
  else
    printf '  \033[31m%-7s\033[0m %-8s %s\n' BLOCKED "$label" "$url"
    FAILED=1
  fi
}

echo "Origin: $ORIGIN"
echo
probe "auth"    "https://auth.openpdfedit.com/v1/credits/entitlement"
probe "gateway" "https://gateway.openpdfedit.com/v1/credits/entitlement"
echo

if [ "$FAILED" = 0 ]; then
  echo "Both servers will answer the extension."
  exit 0
fi

cat <<MSG
A blocked server means the extension will sign in and then fail on every
call. Fix it on the server, not in the extension — no new package is
needed, and republishing will not help.

  auth     deploy/prod.env, extend the existing value:
             OPENAPPS_SERVER_ALLOWED_ORIGINS=<what is there>,$ORIGIN
           then deploy/run.sh

  gateway  deploy/gateway.env, extend the existing value:
             GATEWAY_ALLOWED_ORIGINS=<what is there>,$ORIGIN
           then deploy/run-gateway.sh

Extend. Replacing either value takes the web app offline, and that is a
worse outage than this one.

Re-run this script afterwards. It reads the servers, not the config, so a
pass here is the same thing the reviewer's browser will see.
MSG
exit 1
