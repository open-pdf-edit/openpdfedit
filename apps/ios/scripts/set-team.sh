#!/usr/bin/env bash
# Writes DEVELOPMENT_TEAM into the Xcode project.
#
# The team id is not a secret — it is printed inside every app anyone
# ships, and it appears in the receipt of every purchase. It is kept out
# of the committed project only because this repository builds and tests
# fine without one, and a hard-coded team makes that build fail for
# everyone who is not us.
#
#   ./scripts/set-team.sh            # show what is set
#   ./scripts/set-team.sh ABCDE12345 # set it
#
# Find yours at https://developer.apple.com/account under Membership
# Details, or with:
#
#   security find-identity -v -p codesigning
#
# which prints it in brackets after each identity's name.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT="$(dirname "$SCRIPT_DIR")/OpenPdfEdit.xcodeproj/project.pbxproj"

if [ $# -eq 0 ]; then
  current="$(grep -m1 -o 'DEVELOPMENT_TEAM = [^;]*;' "$PROJECT" | sed 's/DEVELOPMENT_TEAM = //; s/;$//; s/"//g')"
  if [ -z "$current" ]; then
    echo "DEVELOPMENT_TEAM is unset — the app builds for the simulator but cannot be archived."
  else
    echo "DEVELOPMENT_TEAM = $current"
  fi
  exit 0
fi

TEAM="$1"
# Apple team ids are ten alphanumerics. Catching a typo here is cheaper
# than catching it as an opaque provisioning failure ten minutes into an
# archive.
if ! printf '%s' "$TEAM" | grep -Eq '^[A-Z0-9]{10}$'; then
  echo "set-team.sh: '$TEAM' is not a team id (ten upper-case alphanumerics)" >&2
  exit 1
fi

# Only the app target has a DEVELOPMENT_TEAM line; the test target
# inherits signing from the host. Replacing every occurrence is therefore
# both correct and stable against the project being re-saved by Xcode.
perl -0pi -e "s/DEVELOPMENT_TEAM = \"?[A-Z0-9]*\"?;/DEVELOPMENT_TEAM = $TEAM;/g" "$PROJECT"

echo "DEVELOPMENT_TEAM = $TEAM"
