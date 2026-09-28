#!/usr/bin/env bash
# macOS signing, notarization and checks for .github/workflows/release.yml (the `app-macos` and
# `cli-macos` jobs). One file so the release and the rehearsal run exactly the same commands.
#
#   macos-signing.sh preflight                 fail loudly, naming what's missing, before any build
#   macos-signing.sh write-api-key             App Store Connect API key (.p8) → $APPLE_API_KEY_PATH
#   macos-signing.sh import-certificate        Developer ID .p12 → a temporary keychain (CLI job)
#   macos-signing.sh notarize <file> <log>     notarytool submit --wait; fails unless Accepted
#   macos-signing.sh staple <file>             staple the ticket, retrying while Apple publishes it
#   macos-signing.sh verify-app <x.app>        signatures, Gatekeeper verdict, stapled ticket
#   macos-signing.sh verify-dmg <x.dmg>        the same for the disk image and the app inside it
#   macos-signing.sh verify-cli <bin> <log>    signature + the notary ticket covers this binary
#   macos-signing.sh cleanup                   delete the keychain and key files (runs even on failure)
#
# Secrets reach this script only through the environment of the step that needs them. Never add
# `set -x` here: it would print them. Written for bash 3.2 (macOS /bin/bash): no bash 4 features.
set -eo pipefail

# Where the key file, the .p12 and the temporary keychain live while a job runs.
SIGNING_DIR="${RUNNER_TEMP:?RUNNER_TEMP is not set}/pagelamp-signing"
# Public (it's in every signature); the checks make sure our team signed each file.
: "${DEVELOPER_TEAM_ID:?DEVELOPER_TEAM_ID is not set}"

fail() {
  echo "::error::$*"
  exit 1
}

# ---- setup -----------------------------------------------------------------------------------

# The step passes HAS_<secret>=true/false (never the values) plus the non-secret variable.
preflight() {
  local name flag missing=""
  for name in APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_P8; do
    flag="HAS_$name"
    [ "${!flag:-}" = true ] || missing="$missing secret $name,"
  done
  [ -n "${APPLE_SIGNING_IDENTITY:-}" ] || missing="$missing variable APPLE_SIGNING_IDENTITY,"
  if [ -n "$missing" ]; then
    fail "Signing is not set up: missing${missing%,} in the GitHub environment 'release' (Settings → Environments → release). macOS builds are never released unsigned; add them and re-run this job."
  fi
  case "$APPLE_SIGNING_IDENTITY" in
    "Developer ID Application: "*" ($DEVELOPER_TEAM_ID)") ;;
    *) fail "The variable APPLE_SIGNING_IDENTITY must be the certificate's full name, 'Developer ID Application: <name> ($DEVELOPER_TEAM_ID)', as shown by: security find-identity -v -p codesigning" ;;
  esac
  if [ "${REHEARSAL:-false}" = true ] && [ -n "${INPUT_TAG:-}" ]; then
    fail "A rehearsal builds the branch or tag picked under 'Use workflow from'. Leave the 'tag' input empty."
  fi
  echo "Signing setup complete: 5 secrets and APPLE_SIGNING_IDENTITY are present (values not shown)."
}

write_api_key() {
  [ -n "${APPLE_API_KEY_P8:-}" ] || fail "APPLE_API_KEY_P8 is empty"
  [ -n "${APPLE_API_KEY_PATH:-}" ] || fail "APPLE_API_KEY_PATH is not set"
  (umask 077 && mkdir -p "$(dirname "$APPLE_API_KEY_PATH")" && printf '%s\n' "$APPLE_API_KEY_P8" > "$APPLE_API_KEY_PATH")
  grep -q 'BEGIN PRIVATE KEY' "$APPLE_API_KEY_PATH" ||
    fail "APPLE_API_KEY_P8 doesn't look like a .p8 file (expected the whole file, including the BEGIN/END PRIVATE KEY lines)"
  echo "Notary API key written to the runner's temp folder."
}

# The same `security` steps as Tauri's own CI import, but into a keychain whose random password
# exists only inside this function. codesign builds the certificate chain only from keychains on
# the search list, so the keychain is added there too; `cleanup` deletes it again.
import_certificate() {
  local keychain="$SIGNING_DIR/signing.keychain-db" p12="$SIGNING_DIR/certificate.p12" password identities search_list
  [ -n "${APPLE_CERTIFICATE:-}" ] && [ -n "${APPLE_CERTIFICATE_PASSWORD:-}" ] ||
    fail "APPLE_CERTIFICATE or APPLE_CERTIFICATE_PASSWORD is empty"
  (umask 077 && mkdir -p "$SIGNING_DIR")
  password=$(openssl rand -hex 32)
  (umask 077 && printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$p12") ||
    fail "APPLE_CERTIFICATE isn't valid base64 (make it with: base64 -i certificate.p12)"
  security create-keychain -p "$password" "$keychain"
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "$password" "$keychain"
  security import "$p12" -k "$keychain" -f pkcs12 -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null ||
    fail "Couldn't import APPLE_CERTIFICATE (wrong APPLE_CERTIFICATE_PASSWORD, or not a .p12 with its private key?)"
  rm -f "$p12"
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
  search_list=$(security list-keychains -d user | tr -d '"')
  # shellcheck disable=SC2086 # the existing search list, one path per word
  security list-keychains -d user -s "$keychain" $search_list
  # Captured first: with pipefail, `… | grep -q` can fail when grep exits before the writer.
  identities=$(security find-identity -v -p codesigning "$keychain")
  grep -qF "\"$APPLE_SIGNING_IDENTITY\"" <<<"$identities" ||
    fail "APPLE_CERTIFICATE doesn't contain the identity named in APPLE_SIGNING_IDENTITY (or it isn't valid)"
  echo "SIGNING_KEYCHAIN=$keychain" >> "$GITHUB_ENV"
  echo "Certificate imported into a temporary keychain."
}

cleanup() {
  if [ -f "$SIGNING_DIR/signing.keychain-db" ]; then
    security delete-keychain "$SIGNING_DIR/signing.keychain-db" ||
      echo "::warning::Couldn't delete the temporary keychain"
  fi
  rm -rf "$SIGNING_DIR"
  echo "Signing material removed from the runner."
}

# ---- notarization ------------------------------------------------------------------------------

# Submit, wait, download the log, and fail unless Apple says Accepted. The exit code alone isn't
# trusted: the status comes from notarytool's JSON output (Tauri checks it the same way). The
# timeout is generous because a team's first submissions can take longer than the usual minutes.
notarize() {
  local file=$1 log=$2 result status id rc=0
  local -a auth
  [ -f "$file" ] || fail "Nothing to notarize at $file"
  [ -s "${APPLE_API_KEY_PATH:-}" ] || fail "The notary API key file is missing (APPLE_API_KEY_PATH)"
  [ -n "${APPLE_API_KEY:-}" ] && [ -n "${APPLE_API_ISSUER:-}" ] || fail "APPLE_API_KEY or APPLE_API_ISSUER is empty"
  auth=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER")
  mkdir -p "$(dirname "$log")"
  echo "Submitting $(basename "$file") to Apple's notary service (usually a few minutes)..."
  result=$(xcrun notarytool submit "$file" "${auth[@]}" --wait --timeout 2h --output-format json) || rc=$?
  id=$(jq -r '.id // empty' <<<"$result" 2>/dev/null || true)
  status=$(jq -r '.status // empty' <<<"$result" 2>/dev/null || true)
  echo "notarytool exit $rc, submission ${id:-(none)}, status ${status:-(unknown)}"
  if [ -n "$id" ]; then
    xcrun notarytool log "$id" "${auth[@]}" "$log" >/dev/null ||
      echo "::warning::Couldn't download the notary log for submission $id"
  fi
  # Apple: always read the log, even when the submission is accepted; it lists warnings too.
  if [ -f "$log" ]; then
    jq -r '(.issues // [])[] |
      "::\(if .severity == "error" then "error" else "warning" end)::notary \(.severity): \(.path // "-"): \(.message)"' "$log"
  fi
  if [ "$status" != Accepted ]; then
    [ -n "$id" ] || printf '%s\n' "$result"
    fail "Apple did not accept $(basename "$file") (status ${status:-unknown}); see the notary log above and in the job's artifacts"
  fi
  if [ ! -f "$log" ] || ! jq -e '.status == "Accepted"' "$log" >/dev/null; then
    fail "The notary log for $(basename "$file") is missing or doesn't say Accepted"
  fi
  echo "Notarized: $(basename "$file")"
}

# stapler fetches the ticket from Apple's CloudKit, which can lag a little behind Accepted
# ("Record not found", error 65); a later attempt then works. `verify-dmg` checks the result.
staple() {
  local file=$1 attempt
  [ -e "$file" ] || fail "Nothing to staple at $file"
  for attempt in 1 2 3 4 5; do
    xcrun stapler staple "$file" && return 0
    [ "$attempt" = 5 ] || { echo "The ticket isn't available yet; retrying in 30 s..."; sleep 30; }
  done
  fail "Couldn't staple the notarization ticket to $file"
}

# ---- checks -------------------------------------------------------------------------------------

# Developer ID from our team, plus a secure timestamp and the hardened runtime for code (Apple's
# notarization requirements). Containers (the .dmg) only need the Developer ID signature.
check_signature() {
  local path=$1 kind=$2 info
  info=$(codesign --display --verbose=2 "$path" 2>&1) || { echo "$info"; fail "No readable signature on $path"; }
  echo "-- $path"
  printf '%s\n' "$info" | grep -E '^(Identifier|Format|Authority|Timestamp|TeamIdentifier)=|^CodeDirectory ' || true
  grep -q '^Authority=Developer ID Application: ' <<<"$info" ||
    fail "$path isn't signed with a Developer ID Application certificate"
  grep -qx "TeamIdentifier=$DEVELOPER_TEAM_ID" <<<"$info" || fail "$path isn't signed by team $DEVELOPER_TEAM_ID"
  if [ "$kind" = code ]; then
    grep -q '^Timestamp=' <<<"$info" || fail "$path has no secure timestamp"
    grep -Eq '^CodeDirectory .*flags=0x[0-9a-f]+\([^)]*runtime' <<<"$info" ||
      fail "$path isn't signed with the hardened runtime"
  fi
}

# The desktop app's sidecar and the standalone CLI are signed with codesign's default identifier
# (the file name), so both have the same designated requirement: identifier + Team ID.
check_identifier() {
  local info
  info=$(codesign --display --verbose=1 "$1" 2>&1) || true
  grep -qx "Identifier=$2" <<<"$info" || fail "$1 isn't signed with the identifier $2"
}

gatekeeper() {
  local path=$1 type=$2 out
  if [ "$type" = open ]; then
    # Apple's recommended check for a disk image: assess its own (primary) signature.
    out=$(spctl --assess --type open --context context:primary-signature -vv "$path" 2>&1) || {
      echo "$out"
      fail "Gatekeeper rejects $path"
    }
  else
    out=$(spctl --assess --type execute -vv "$path" 2>&1) || {
      echo "$out"
      fail "Gatekeeper rejects $path"
    }
  fi
  echo "$out"
  grep -qx 'source=Notarized Developer ID' <<<"$out" || {
    spctl --status || true
    fail "Gatekeeper doesn't report $path as 'Notarized Developer ID'"
  }
}

stapled() {
  xcrun stapler validate "$1" || fail "No valid notarization ticket is stapled to $1"
}

verify_app() {
  local app=$1 bin
  [ -d "$app" ] || fail "No app bundle at $app"
  echo "== $app"
  codesign --verify --deep --strict --verbose=2 "$app" || fail "codesign --verify --deep --strict failed for $app"
  check_signature "$app" code
  # Tauri signs every executable in Contents/MacOS (the app and the `pagelamp` sidecar) first.
  for bin in "$app"/Contents/MacOS/*; do
    check_signature "$bin" code
  done
  check_identifier "$app/Contents/MacOS/pagelamp" pagelamp
  gatekeeper "$app" execute
  stapled "$app"
}

MOUNT_POINT=""
detach_dmg() {
  if [ -n "$MOUNT_POINT" ]; then
    hdiutil detach -quiet "$MOUNT_POINT" || hdiutil detach -quiet -force "$MOUNT_POINT" || true
    MOUNT_POINT=""
  fi
}

verify_dmg() {
  local dmg=$1 app attempt
  [ -f "$dmg" ] || fail "No disk image at $dmg"
  echo "== $dmg"
  codesign --verify --strict --verbose=2 "$dmg" || fail "codesign --verify --strict failed for $dmg"
  check_signature "$dmg" container
  gatekeeper "$dmg" open
  stapled "$dmg"
  # And what people actually install: the app inside the disk image must carry its own ticket.
  MOUNT_POINT=$(mktemp -d "$RUNNER_TEMP/dmg.XXXXXX")
  trap detach_dmg EXIT
  # hdiutil on CI runners sometimes reports "Resource busy" for a moment; try a few times.
  for attempt in 1 2 3; do
    hdiutil attach -nobrowse -readonly -noautoopen -mountpoint "$MOUNT_POINT" "$dmg" >/dev/null && break
    [ "$attempt" = 3 ] && fail "Couldn't mount $dmg"
    sleep 5
  done
  for app in "$MOUNT_POINT"/*.app; do break; done
  [ -d "$app" ] || fail "No app inside $dmg"
  verify_app "$app"
  detach_dmg
  trap - EXIT
}

# A bare binary can't be stapled and Gatekeeper's spctl check only understands apps, so: the
# signature, the notary log listing this exact binary (by cdhash), and Apple's online check.
verify_cli() {
  local bin=$1 log=$2 info cdhash attempt
  [ -f "$bin" ] || fail "No binary at $bin"
  echo "== $bin"
  codesign --verify --strict --verbose=2 "$bin" || fail "codesign --verify --strict failed for $bin"
  check_signature "$bin" code
  check_identifier "$bin" pagelamp
  info=$(codesign --display --verbose=3 "$bin" 2>&1) || true
  cdhash=$(sed -n 's/^CDHash=//p' <<<"$info")
  [ -n "$cdhash" ] || fail "Couldn't read the cdhash of $bin"
  [ -f "$log" ] || fail "No notary log at $log"
  jq -e --arg h "$cdhash" \
    '.status == "Accepted" and any(.ticketContents[]?; (.cdhash | ascii_downcase) == ($h | ascii_downcase))' \
    "$log" >/dev/null || fail "The notary log doesn't list this binary (cdhash $cdhash)"
  echo "The notary ticket covers $bin (cdhash $cdhash)."
  # The ticket can take a moment to be published after Accepted.
  for attempt in 1 2 3 4 5 6 7 8 9; do
    if codesign --verify --strict --verbose=2 --check-notarization --test-requirement '=notarized' "$bin"; then
      echo "Apple's online notarization check passes for $bin."
      return 0
    fi
    [ "$attempt" = 9 ] || { echo "Ticket not visible online yet; retrying in 20 s..."; sleep 20; }
  done
  fail "codesign --check-notarization can't find a notarization ticket for $bin"
}

cmd=${1:-}
[ $# -gt 0 ] && shift
case "$cmd" in
  preflight) preflight ;;
  write-api-key) write_api_key ;;
  import-certificate) import_certificate ;;
  notarize) notarize "$@" ;;
  staple) staple "$@" ;;
  verify-app) verify_app "$@" ;;
  verify-dmg) verify_dmg "$@" ;;
  verify-cli) verify_cli "$@" ;;
  cleanup) cleanup ;;
  *)
    echo "usage: $0 preflight|write-api-key|import-certificate|notarize|staple|verify-app|verify-dmg|verify-cli|cleanup" >&2
    exit 2
    ;;
esac
