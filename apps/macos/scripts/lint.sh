#!/usr/bin/env bash
# Lint for the Mac app (CI and before every commit):
#   1. glass only in Sources/PageLamp/Chrome/ (spec §1.3): glassEffect*, GlassEffectContainer,
#      AppKit's NSGlassEffectView / NSGlassEffectContainerView and the glass button styles appear
#      nowhere else;
#   2. the generated tokens and strings are up to date (gen-tokens / gen-strings --check);
#   3. every string key used in Swift exists, and no UI text is hard-coded
#      (scripts/check-swift-strings.mjs).
#
#   apps/macos/scripts/lint.sh
#
# Needs Node ≥ 24 only (no Swift build). Exit 1 if anything fails; every check runs.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MACOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT="$(cd "$MACOS_DIR/../.." && pwd)"
failures=0

step() { echo "==> $*"; }
failed() {
  echo "lint: FAILED: $*" >&2
  failures=$((failures + 1))
}

step "glass only in Sources/PageLamp/Chrome/"
GLASS='glassEffect|GlassEffectContainer|NSGlassEffect|buttonStyle\([[:space:]]*\.glass|\.glassProminent|GlassButtonStyle|GlassProminentButtonStyle'
glass_hits="$(grep -rnE --include='*.swift' "$GLASS" "$MACOS_DIR/Sources" \
  | grep -v "^$MACOS_DIR/Sources/PageLamp/Chrome/" || true)"
if [[ -n "$glass_hits" ]]; then
  echo "$glass_hits" | sed "s|^$ROOT/||" >&2
  failed "glass APIs outside Sources/PageLamp/Chrome/ (content never uses glass)"
else
  echo "    ok ($(grep -rlE --include='*.swift' "$GLASS" "$MACOS_DIR/Sources/PageLamp/Chrome" | wc -l | tr -d ' ') Chrome files use glass)"
fi

step "design tokens up to date"
node "$ROOT/design/tokens/gen-tokens.mjs" --check || failed "tokens are stale: run node design/tokens/gen-tokens.mjs"

step "strings up to date"
node "$MACOS_DIR/scripts/gen-strings.mjs" --check || failed "strings are stale: run node apps/macos/scripts/gen-strings.mjs"

step "string keys used in Swift exist; no hard-coded UI text"
node "$MACOS_DIR/scripts/check-swift-strings.mjs" --root "$ROOT" || failed "string check"

if ((failures > 0)); then
  echo "lint: $failures check(s) failed" >&2
  exit 1
fi
echo "lint: all checks passed"
