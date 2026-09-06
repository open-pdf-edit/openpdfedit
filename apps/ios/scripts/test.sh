#!/usr/bin/env bash
# Builds and tests the iOS app in a simulator.
#
# Prefers an iOS 17 runtime when one is installed. The StoreKit test
# service ("Octane") in the iOS 26.5 simulator runtime accepts a
# configuration and then answers product requests from the real Media API
# anyway, so every purchase test sees an empty catalogue — visible in
# storekitd's own log, which shows the configuration saved and the request
# going out to the network regardless. The same tests pass on iOS 17.0,
# which is also the app's deployment target. The app itself is unaffected:
# this is the simulator's test harness, not StoreKit.
#
# On a machine with no iOS 17 runtime — a CI runner has whatever its image
# shipped with — this falls back to the newest available iPhone, and the
# purchase tests skip themselves loudly rather than failing. See
# OpenPdfEditTests/LocalStoreKit.swift for why that skip cannot hide a
# regression in this app.
#
# Override the whole thing with IOS_TEST_DESTINATION.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(dirname "$SCRIPT_DIR")"

[ -d "$IOS_DIR/www" ] || {
  echo "test.sh: apps/ios/www is missing — run apps/ios/scripts/sync-web.sh" >&2
  exit 1
}

pick_destination() {
  xcrun simctl list devices available -j | python3 -c '
import json, re, sys

runtimes = json.load(sys.stdin)["devices"]

def version(runtime):
    match = re.search(r"iOS-(\d+)-(\d+)", runtime)
    return (int(match.group(1)), int(match.group(2))) if match else None

candidates = []
for runtime, devices in runtimes.items():
    parsed = version(runtime)
    if not parsed:
        continue
    for device in devices:
        if not device.get("isAvailable") or "iPhone" not in device["name"]:
            continue
        # An iOS 17 runtime first, then the newest of anything else.
        candidates.append(((0 if parsed[0] == 17 else 1, [-parsed[0], -parsed[1]]), device["udid"]))

if not candidates:
    sys.exit("no available iPhone simulator")
candidates.sort(key=lambda c: (c[0][0], c[0][1]))
print(candidates[0][1])
'
}

DESTINATION="${IOS_TEST_DESTINATION:-platform=iOS Simulator,id=$(pick_destination)}"
echo "==> $DESTINATION"

exec xcodebuild test \
  -project "$IOS_DIR/OpenPdfEdit.xcodeproj" \
  -scheme OpenPdfEdit \
  -destination "$DESTINATION" \
  -parallel-testing-enabled NO \
  "$@"
