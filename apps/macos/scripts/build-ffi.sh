#!/usr/bin/env bash
# Build the Rust core for the macOS app: crates/pagelamp-ffi as a static library, its Swift
# bindings, and the xcframework SwiftPM links (apps/macos/Package.swift).
#
#   apps/macos/scripts/build-ffi.sh
#
# Outputs (all generated, git-ignored; re-running replaces them):
#   apps/macos/Sources/PageLampKit/Generated/pagelamp_ffi.swift   bindings (module PageLampKit)
#   apps/macos/Frameworks/PageLampFFI.xcframework                   libpagelamp_ffi.a + C header
#                                                                  + module.modulemap (pagelamp_ffiFFI)
# The bindings and the library must always come from the same build: the Swift side checks
# UniFFI's contract version and every function's checksum when the library loads.
#
# Needs: rustup/cargo (the aarch64-apple-darwin target), Xcode (xcodebuild -create-xcframework).
# Apple silicon only: the app targets macOS 26+, arm64.
set -euo pipefail

fail() {
  echo "build-ffi: error: $*" >&2
  exit 1
}
trap 'echo "build-ffi: FAILED (line $LINENO)" >&2' ERR

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MACOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT="$(cd "$MACOS_DIR/../.." && pwd)"
[[ -f "$ROOT/crates/pagelamp-ffi/Cargo.toml" ]] || fail "crates/pagelamp-ffi not found under $ROOT"

TARGET="aarch64-apple-darwin"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
LIB="$TARGET_DIR/$TARGET/release/libpagelamp_ffi.a"
BINDGEN="$TARGET_DIR/debug/uniffi-bindgen-swift"
GENERATED="$MACOS_DIR/Sources/PageLampKit/Generated"
XCFRAMEWORK="$MACOS_DIR/Frameworks/PageLampFFI.xcframework"
MODULE="pagelamp_ffiFFI"

# The Swift app's deployment target: C code in dependencies (SQLite, …) is compiled for it too,
# otherwise the linker warns "built for newer macOS version".
export MACOSX_DEPLOYMENT_TARGET=26.0

if ! command -v cargo >/dev/null 2>&1; then
  # shellcheck disable=SC1091
  [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
fi
command -v cargo >/dev/null 2>&1 || fail "cargo not found (install Rust with rustup)"

# xcodebuild needs a full Xcode; use /Applications/Xcode.app when only the Command Line Tools
# are selected (without changing the system-wide selection).
if [[ -z "${DEVELOPER_DIR:-}" ]] && [[ "$(xcode-select -p 2>/dev/null)" == *CommandLineTools* ]] \
  && [[ -d /Applications/Xcode.app/Contents/Developer ]]; then
  export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
fi
xcodebuild -version >/dev/null 2>&1 || fail "xcodebuild is not usable; install Xcode or set DEVELOPER_DIR"

[[ "$(uname -m)" == "arm64" ]] || fail "build on an Apple silicon Mac (arm64)"

seconds() { date +%s; }
START=$(seconds)
cd "$ROOT"

echo "==> cargo build --release -p pagelamp-ffi --lib --target $TARGET (MACOSX_DEPLOYMENT_TARGET=$MACOSX_DEPLOYMENT_TARGET)"
T=$(seconds)
cargo build --release --locked -p pagelamp-ffi --lib --target "$TARGET"
[[ -f "$LIB" ]] || fail "expected $LIB after the build"
echo "    $(( $(seconds) - T )) s"

echo "==> cargo build -p pagelamp-ffi --features bindgen --bin uniffi-bindgen-swift"
T=$(seconds)
# A host tool: built like `cargo test` builds (debug, no deployment target), so the two share
# their compiled dependencies instead of rebuilding each other's.
env -u MACOSX_DEPLOYMENT_TARGET cargo build --locked -p pagelamp-ffi --features bindgen --bin uniffi-bindgen-swift
[[ -x "$BINDGEN" ]] || fail "expected $BINDGEN after the build"
echo "    $(( $(seconds) - T )) s"

echo "==> Swift bindings -> ${GENERATED#"$ROOT/"}"
rm -rf "$GENERATED"
mkdir -p "$GENERATED"
"$BINDGEN" --swift-sources "$LIB" "$GENERATED"
[[ -f "$GENERATED/pagelamp_ffi.swift" ]] || fail "bindgen did not write pagelamp_ffi.swift"
# Only the Swift sources belong in the module; headers/modulemaps go to the xcframework.
find "$GENERATED" -type f ! -name '*.swift' -delete

HEADERS="$(mktemp -d "${TMPDIR:-/tmp}/pagelamp-ffi-headers.XXXXXX")"
cleanup() { rm -rf "$HEADERS"; }
trap cleanup EXIT
"$BINDGEN" --headers "$LIB" "$HEADERS"
# A plain `module` (not `framework module`): the xcframework holds a static library.
"$BINDGEN" --modulemap --module-name "$MODULE" --modulemap-filename module.modulemap "$LIB" "$HEADERS"
[[ -f "$HEADERS/$MODULE.h" ]] || fail "bindgen did not write $MODULE.h"
[[ -f "$HEADERS/module.modulemap" ]] || fail "bindgen did not write module.modulemap"
grep -q "^module $MODULE " "$HEADERS/module.modulemap" || fail "unexpected module map: $(cat "$HEADERS/module.modulemap")"

echo "==> xcodebuild -create-xcframework -> ${XCFRAMEWORK#"$ROOT/"}"
rm -rf "$XCFRAMEWORK"
mkdir -p "$(dirname "$XCFRAMEWORK")"
xcodebuild -create-xcframework -library "$LIB" -headers "$HEADERS" -output "$XCFRAMEWORK" >/dev/null
SLICE_LIB="$XCFRAMEWORK/macos-arm64/libpagelamp_ffi.a"
[[ -f "$SLICE_LIB" ]] || fail "xcframework has no macos-arm64/libpagelamp_ffi.a"
[[ -f "$XCFRAMEWORK/macos-arm64/Headers/module.modulemap" ]] || fail "xcframework has no module map"

# Every object must target macOS <= 26.0 (else the app links with "built for newer macOS").
NEWEST_MINOS="$(otool -l "$SLICE_LIB" | awk '$1 == "minos" { print $2 }' | sort -t. -k1,1n -k2,2n | tail -1)"
case "$NEWEST_MINOS" in
  "" | 26.0 | 2[0-5].* | 1[0-9].*) ;;
  *) fail "an object in the library targets macOS $NEWEST_MINOS (> 26.0)" ;;
esac

echo "==> done in $(( $(seconds) - START )) s"
echo "    library:   $(du -h "$SLICE_LIB" | cut -f1)  ($(stat -f %z "$SLICE_LIB") bytes, $(lipo -archs "$SLICE_LIB"), newest minos ${NEWEST_MINOS:-?})"
echo "    bindings:  $(wc -l < "$GENERATED/pagelamp_ffi.swift" | tr -d ' ') lines  ${GENERATED#"$ROOT/"}/pagelamp_ffi.swift"
echo "    framework: $(du -sh "$XCFRAMEWORK" | cut -f1)  ${XCFRAMEWORK#"$ROOT/"}"
