#!/usr/bin/env bash
# macOS signing, notarization and checks for .github/workflows/release.yml (the `app-macos` and
# `cli-macos` jobs). One file so the release and the rehearsal run exactly the same commands.
#
#   macos-signing.sh preflight                 fail loudly, naming what's missing, before any build
#   macos-signing.sh write-api-key             App Store Connect API key (.p8) → $APPLE_API_KEY_PATH
#   macos-signing.sh import-certificate        Developer ID .p12 → a temporary keychain (CLI job)
#   macos-signing.sh notarize <file> <log>     upload, poll until Apple is done; fails unless Accepted
#   macos-signing.sh staple <file>             staple the ticket, retrying while Apple publishes it
#   macos-signing.sh verify-app <x.app>        signatures, Gatekeeper verdict, stapled ticket, universal
#   macos-signing.sh verify-dmg <x.dmg>        the same for the disk image and the app inside it
#   macos-signing.sh verify-updater-archive <x.app.tar.gz>
#                                              the same for the app inside the updater's archive
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

# Upload, wait for Apple's verdict, download the log, and fail unless Apple says Accepted.
#
# Not `notarytool submit --wait`: that keeps asking over one run of notarytool for the whole wait,
# and in rehearsal run 36373246891 (a new team's first submissions queued for almost two hours)
# one network drop on the runner ("NSURLErrorDomain Code=-1009 ... offline") failed the job
# although Apple had the submission. So: upload without waiting (retried on network and server
# errors, 4 attempts; an upload that broke off can leave an unfinished submission behind, which
# does no harm), then ask `notarytool info` every NOTARY_POLL_SECONDS (default 30), riding out
# network and server errors, until the status is final or NOTARY_TIMEOUT_MINUTES (default 240,
# counted from the upload) have passed. GitHub stops any job on its hosted runners after 6 hours
# whatever timeout-minutes says, so the build, this wait and, in `app-macos`, Tauri's own
# notarization of the .app must fit in that together. Tauri's notarization runs inside `tauri
# bundle` as one `notarytool submit --wait` and can't be made to retry from here: if the network
# drops during it, that step fails, and "Re-run failed jobs" builds, signs and submits again.
#
# The exit code alone isn't trusted: the status comes from notarytool's JSON output (Tauri checks
# it the same way). Accepted, Invalid and Rejected are final; anything else ("In Progress") isn't.
NOTARY_AUTH=()
NOTARY_STDERR=""
NOTARY_POLL=30

# Runs `xcrun notarytool <args>` with the API key and JSON output. Sets NT_RC (the exit code),
# NT_JSON (stdout, notarytool's answer) and NT_TEXT (stdout and stderr, to show and to tell a
# network hiccup from a real error). The command line itself is never printed.
notarytool_json() {
  NT_RC=0
  NT_JSON=$(xcrun notarytool "$@" "${NOTARY_AUTH[@]}" --output-format json 2>"$NOTARY_STDERR") || NT_RC=$?
  NT_TEXT=$({ printf '%s\n' "$NT_JSON"; cat "$NOTARY_STDERR"; } | sed '/^[[:space:]]*$/d')
}

# A field of notarytool's JSON answer, or nothing.
nt_field() {
  jq -r --arg k "$1" 'if type == "object" then .[$k] // empty else empty end' <<<"$NT_JSON" 2>/dev/null || true
}

# Worth another try: no connection or a dropped one (NSURLErrorDomain: -1009 offline, -1001 timed
# out, -1005 connection lost, ...; notarytool then reports "statusCode: nil"), or the service is
# busy or failing (HTTP 5xx, 408, 429). Anything else (a wrong key, a missing agreement, a bad
# file) won't fix itself.
transient_error() {
  grep -Eqi 'NSURLErrorDomain|statusCode: nil|(status ?code:? *(Optional\()?)(5[0-9][0-9]|408|429)([^0-9]|$)|timed out|offline|network connection was lost|could not connect|connection (was )?(reset|refused)|service unavailable|bad gateway|gateway time-?out|internal server error|too many requests' <<<"$1"
}

minutes_since() {
  echo "$(((SECONDS - $1) / 60))"
}

# Uploads $1 and sets NOTARY_ID.
notary_submit() {
  local file=$1 attempt
  for attempt in 1 2 3 4; do
    notarytool_json submit "$file"
    NOTARY_ID=$(nt_field id)
    if [ "$NT_RC" = 0 ] && [ -n "$NOTARY_ID" ]; then
      return 0
    fi
    printf '%s\n' "$NT_TEXT"
    [ -z "$NOTARY_ID" ] || echo "(That attempt left submission $NOTARY_ID unfinished; it is not used.)"
    NOTARY_ID=""
    if [ "$attempt" = 4 ] || ! transient_error "$NT_TEXT"; then
      fail "Couldn't upload $(basename "$file") to Apple's notary service (notarytool exit $NT_RC, attempt $attempt of 4); see the message above"
    fi
    echo "Network or server error; uploading again in $((attempt * NOTARY_POLL)) s..."
    sleep "$((attempt * NOTARY_POLL))"
  done
}

# Asks for the status of submission $1 until it's final or it's past $2 (in $SECONDS; $3 is when
# the upload started). Sets NOTARY_STATUS (the last status Apple gave, if any) and NOTARY_PROBLEM
# (why it stopped without a final status).
notary_wait() {
  local id=$1 deadline=$2 started=$3 status last="" errors=0 offline=0 note_at
  NOTARY_STATUS="" NOTARY_PROBLEM=""
  note_at=$((SECONDS + 600))
  while :; do
    notarytool_json info "$id"
    status=$(nt_field status)
    if [ -n "$status" ]; then
      errors=0 offline=0 NOTARY_STATUS=$status
      if [ "$status" != "$last" ] || [ "$SECONDS" -ge "$note_at" ]; then
        echo "[$(minutes_since "$started") min] $status"
        last=$status note_at=$((SECONDS + 600))
      fi
      case "$status" in Accepted | Invalid | Rejected) return 0 ;; esac
    elif transient_error "$NT_TEXT"; then
      # The full message once per outage, then one line per attempt.
      [ "$offline" -gt 0 ] || printf '%s\n' "$NT_TEXT"
      offline=$((offline + 1))
      echo "[$(minutes_since "$started") min] Can't reach Apple's notary service (network or server error, $offline in a row); asking again."
    else
      printf '%s\n' "$NT_TEXT"
      errors=$((errors + 1))
      if [ "$errors" -ge 5 ]; then
        NOTARY_PROBLEM="notarytool info failed $errors times in a row (exit $NT_RC; see above)"
        return 0
      fi
      echo "[$(minutes_since "$started") min] Unexpected answer from notarytool info (exit $NT_RC, $errors in a row); asking again."
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
      NOTARY_PROBLEM="no final status after $(minutes_since "$started") min (the limit is NOTARY_TIMEOUT_MINUTES=$NOTARY_LIMIT)"
      return 0
    fi
    sleep "$NOTARY_POLL"
  done
}

# Downloads the log of submission $1 to $2 (the file the verify steps read and the job keeps as
# an artifact), up to $3 attempts: it can lag a moment behind the final status.
notary_log() {
  local id=$1 log=$2 tries=$3 attempt=1
  while :; do
    rm -f "$log"
    notarytool_json log "$id" "$log"
    if [ "$NT_RC" = 0 ] && jq -e 'type == "object"' "$log" >/dev/null 2>&1; then
      return 0
    fi
    printf '%s\n' "$NT_TEXT"
    [ "$attempt" -lt "$tries" ] || break
    attempt=$((attempt + 1))
    sleep "$NOTARY_POLL"
  done
  rm -f "$log"
  echo "::warning::Couldn't download the notary log for submission $id"
}

notarize() {
  local file=$1 log=$2 name started
  [ -f "$file" ] || fail "Nothing to notarize at $file"
  [ -n "$log" ] || fail "usage: macos-signing.sh notarize <file> <log>"
  [ -s "${APPLE_API_KEY_PATH:-}" ] || fail "The notary API key file is missing (APPLE_API_KEY_PATH)"
  [ -n "${APPLE_API_KEY:-}" ] && [ -n "${APPLE_API_ISSUER:-}" ] || fail "APPLE_API_KEY or APPLE_API_ISSUER is empty"
  NOTARY_LIMIT=${NOTARY_TIMEOUT_MINUTES:-240}
  NOTARY_POLL=${NOTARY_POLL_SECONDS:-30}
  case "$NOTARY_LIMIT" in '' | *[!0-9]*) fail "NOTARY_TIMEOUT_MINUTES must be a whole number of minutes, not '$NOTARY_LIMIT'" ;; esac
  case "$NOTARY_POLL" in '' | *[!0-9]*) fail "NOTARY_POLL_SECONDS must be a whole number of seconds, not '$NOTARY_POLL'" ;; esac
  NOTARY_AUTH=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER")
  mkdir -p "$(dirname "$log")"
  NOTARY_STDERR=$(mktemp "$RUNNER_TEMP/notarytool-stderr.XXXXXX")
  trap 'rm -f "$NOTARY_STDERR"' EXIT
  name=$(basename "$file")
  started=$SECONDS

  echo "Uploading $name to Apple's notary service..."
  notary_submit "$file"
  echo "::notice::$name is Apple notary submission $NOTARY_ID"
  echo "Waiting for Apple's verdict, asking every $NOTARY_POLL s for up to $NOTARY_LIMIT min (usually a few minutes; a team's first submissions can take hours)..."
  notary_wait "$NOTARY_ID" "$((started + NOTARY_LIMIT * 60))" "$started"

  # Apple: always read the log, even when the submission is accepted; it lists warnings too.
  # Before the status is final there is no log yet, but try anyway.
  case "$NOTARY_STATUS" in
    Accepted | Invalid | Rejected) notary_log "$NOTARY_ID" "$log" 5 ;;
    *) notary_log "$NOTARY_ID" "$log" 1 ;;
  esac
  if [ -f "$log" ]; then
    jq -r '(.issues // [])[] |
      "::\(if .severity == "error" then "error" else "warning" end)::notary \(.severity): \(.path // "-"): \(.message)"' "$log" ||
      echo "::warning::Couldn't read the issues in the notary log"
  fi
  case "$NOTARY_STATUS" in
    Accepted) ;;
    Invalid | Rejected)
      if [ -f "$log" ]; then
        echo "-- notary log for submission $NOTARY_ID"
        cat "$log"
        echo
      fi
      fail "Apple did not accept $name (status $NOTARY_STATUS, submission $NOTARY_ID); see the notary log above and in the job's artifacts"
      ;;
    *)
      fail "No verdict from Apple for $name (submission $NOTARY_ID, last status ${NOTARY_STATUS:-unknown}): $NOTARY_PROBLEM. Apple may still finish it, but this job stops here; re-run the failed job to build, sign and submit again. (To look it up: xcrun notarytool info $NOTARY_ID with the same API key.)"
      ;;
  esac
  if [ ! -f "$log" ] || ! jq -e '.status == "Accepted"' "$log" >/dev/null; then
    fail "The notary log for $name is missing or doesn't say Accepted"
  fi
  echo "Notarized: $name (submission $NOTARY_ID)"
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

# The one macOS download is universal (D4): every executable needs both slices, arm64 (Apple
# silicon) and x86_64 (Intel), and nothing else.
check_universal() {
  local path=$1 archs
  archs=$(lipo -archs "$path" 2>&1) || { echo "$archs"; fail "lipo can't read the architectures of $path"; }
  # shellcheck disable=SC2086 # split lipo's space-separated list
  archs=$(printf '%s\n' $archs | sort | tr '\n' ' ')
  [ "$archs" = "arm64 x86_64 " ] || fail "$path isn't universal (arm64 and x86_64): lipo -archs says '${archs% }'"
  echo "$path: ${archs% }"
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
    check_universal "$bin"
  done
  check_identifier "$app/Contents/MacOS/pagelamp" pagelamp
  gatekeeper "$app" execute
  stapled "$app"
}

# The updater installs the app from its .app.tar.gz, not from the .dmg, so the app in there must
# pass the same checks: Tauri archives it after notarizing and stapling it.
verify_updater_archive() {
  local archive=$1 dir app count=0
  [ -f "$archive" ] || fail "No updater archive at $archive"
  dir=$(mktemp -d "$RUNNER_TEMP/updater-archive.XXXXXX")
  tar -xzf "$archive" -C "$dir" || fail "Couldn't extract $archive"
  for app in "$dir"/*.app; do
    [ -d "$app" ] && count=$((count + 1))
  done
  [ "$count" = 1 ] || fail "$archive should hold exactly one .app at its top level (found $count)"
  for app in "$dir"/*.app; do
    echo "== $archive"
    verify_app "$app"
  done
  rm -rf "$dir"
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
  verify-updater-archive) verify_updater_archive "$@" ;;
  verify-cli) verify_cli "$@" ;;
  cleanup) cleanup ;;
  *)
    echo "usage: $0 preflight|write-api-key|import-certificate|notarize|staple|verify-app|verify-dmg|verify-updater-archive|verify-cli|cleanup" >&2
    exit 2
    ;;
esac
