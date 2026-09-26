#!/usr/bin/env bash
# Build the signed preview app: apps/macos/dist/PageLamp Preview.app
#
#   apps/macos/scripts/build-app.sh
#
# Steps:
#   1. the Rust core for Swift (scripts/build-ffi.sh) if the xcframework or bindings are missing
#      or older than any crate source, Cargo.toml or Cargo.lock;
#   2. the CLI sidecar: cargo build --release -p pagelamp-cli (Contents/MacOS/pagelamp, what AI
#      apps launch for the MCP server);
#   3. swift build -c release --product PageLampApp, with the SDK version stamped explicitly
#      (SwiftPM otherwise records sdk = deployment target and AppKit falls back to pre-26 metrics);
#   4. the bundle: Info.plist, the SwiftPM resource bundle (strings) plus copies of its en and
#      zh-Hans .lproj folders in Contents/Resources (so the system menus localize), the icon,
#      the sidecar;
#   5. ad-hoc signing inside out (sidecar, resource bundle, app; hardened runtime), then
#      `codesign --verify --deep --strict` and a check that the executable's SDK stamp is >= 26.
#
# Never launches the app. Ad-hoc signed: Gatekeeper refuses it on other Macs (fine for a preview).
# Needs: rustup/cargo (aarch64-apple-darwin), Xcode (DEVELOPER_DIR is set to it when only the
# Command Line Tools are selected).
set -euo pipefail

fail() {
  echo "build-app: error: $*" >&2
  exit 1
}
trap 'echo "build-app: FAILED (line $LINENO)" >&2' ERR

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MACOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT="$(cd "$MACOS_DIR/../.." && pwd)"

APP_NAME="PageLamp Preview"
BUNDLE_ID="dev.pagelamp.mac-preview"
EXECUTABLE="PageLampApp"
MIN_MACOS="26.0"
TARGET="aarch64-apple-darwin"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
XCF_LIB="$MACOS_DIR/Frameworks/PageLampFFI.xcframework/macos-arm64/libpagelamp_ffi.a"
BINDINGS="$MACOS_DIR/Sources/PageLampKit/Generated/pagelamp_ffi.swift"
CLI="$TARGET_DIR/$TARGET/release/pagelamp"
ICON="$ROOT/apps/desktop/src-tauri/icons/icon.icns"
DIST="$MACOS_DIR/dist"
APP="$DIST/$APP_NAME.app"

[[ "$(uname -m)" == "arm64" ]] || fail "build on an Apple silicon Mac (arm64)"
[[ -f "$ICON" ]] || fail "missing icon $ICON"

if ! command -v cargo >/dev/null 2>&1; then
  # shellcheck disable=SC1091
  [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
fi
command -v cargo >/dev/null 2>&1 || fail "cargo not found (install Rust with rustup)"

# Xcode's toolchain (SwiftUI macros need it) without changing the system-wide selection.
if [[ -z "${DEVELOPER_DIR:-}" ]] && [[ "$(xcode-select -p 2>/dev/null)" == *CommandLineTools* ]] \
  && [[ -d /Applications/Xcode.app/Contents/Developer ]]; then
  export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
fi
xcrun --find swift >/dev/null 2>&1 || fail "no Swift toolchain; install Xcode or set DEVELOPER_DIR"

seconds() { date +%s; }
START=$(seconds)

# ── 1. Rust core for Swift ──────────────────────────────────────────────────────────────────
ffi_reason=""
if [[ ! -f "$XCF_LIB" || ! -f "$BINDINGS" ]]; then
  ffi_reason="missing"
else
  # The bindings are rewritten on every build-ffi.sh run, so their mtime is "last generated".
  # (The xcframework's library keeps cargo's mtime, which a no-op cargo build doesn't bump.)
  newer="$(find "$ROOT/crates" "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" -newer "$BINDINGS" \
    \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' -o -name 'uniffi.toml' -o -name '*.sql' \) \
    -print -quit)"
  [[ -n "$newer" ]] && ffi_reason="stale (${newer#"$ROOT/"} is newer)"
fi
if [[ -n "$ffi_reason" ]]; then
  echo "==> Rust core for Swift is $ffi_reason: scripts/build-ffi.sh"
  "$SCRIPT_DIR/build-ffi.sh"
else
  echo "==> Rust core for Swift is up to date"
fi

# ── 2. CLI sidecar ──────────────────────────────────────────────────────────────────────────
echo "==> cargo build --release -p pagelamp-cli --target $TARGET"
T=$(seconds)
# Same target and deployment target as the ffi build, so the two share compiled dependencies.
(cd "$ROOT" && MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS" cargo build --release --locked -p pagelamp-cli --target "$TARGET")
[[ -x "$CLI" ]] || fail "expected $CLI after the build"
echo "    $(( $(seconds) - T )) s"

# ── 3. Swift app ────────────────────────────────────────────────────────────────────────────
SDK_VERSION="$(xcrun --sdk macosx --show-sdk-version)"
echo "==> swift build -c release --product $EXECUTABLE (platform_version macos $MIN_MACOS sdk $SDK_VERSION)"
T=$(seconds)
SWIFT_FLAGS=(-c release -Xlinker -platform_version -Xlinker macos -Xlinker "$MIN_MACOS" -Xlinker "$SDK_VERSION")
(cd "$MACOS_DIR" && swift build "${SWIFT_FLAGS[@]}" --product "$EXECUTABLE")
BIN_DIR="$(cd "$MACOS_DIR" && swift build "${SWIFT_FLAGS[@]}" --show-bin-path)"
[[ -x "$BIN_DIR/$EXECUTABLE" ]] || fail "expected $BIN_DIR/$EXECUTABLE after the build"
RESOURCES_BUNDLE="$BIN_DIR/PageLamp_PageLamp.bundle"
[[ -d "$RESOURCES_BUNDLE" ]] || fail "expected the resource bundle $RESOURCES_BUNDLE"
echo "    $(( $(seconds) - T )) s"

# ── 4. Bundle ───────────────────────────────────────────────────────────────────────────────
# CFBundleShortVersionString: the numeric part of the workspace version (0.1.0-beta.1 → 0.1.0).
WORKSPACE_VERSION="$(awk '
  /^\[workspace\.package\]/ { section = 1; next }
  /^\[/ { section = 0 }
  section && $1 == "version" { gsub(/"/, "", $3); print $3; exit }
' "$ROOT/Cargo.toml")"
SHORT_VERSION="${WORKSPACE_VERSION%%[-+]*}"
[[ "$SHORT_VERSION" =~ ^[0-9]+(\.[0-9]+){0,2}$ ]] || fail "unexpected workspace version '$WORKSPACE_VERSION'"
# CFBundleVersion: the commit count (monotonic on one branch); 1 outside a git checkout.
BUILD_NUMBER="$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)"

echo "==> ${APP#"$ROOT/"} ($SHORT_VERSION, build $BUILD_NUMBER)"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/$EXECUTABLE" "$APP/Contents/MacOS/$EXECUTABLE"
cp "$CLI" "$APP/Contents/MacOS/pagelamp"
cp -R "$RESOURCES_BUNDLE" "$APP/Contents/Resources/"
# The main bundle's own .lproj folders: AppKit localizes what the system provides (the Edit,
# View and Window menus, the sidebar/toolbar items, standard panels) only in the languages the
# *main* bundle has .lproj folders for; CFBundleLocalizations alone is not enough. The app's own
# text still comes from the SwiftPM bundle (Bundle.module); these are copies of its tables.
for language in en zh-Hans; do
  lproj="$RESOURCES_BUNDLE/Contents/Resources/$language.lproj"
  [[ -d "$lproj" ]] || fail "expected $lproj in the resource bundle"
  cp -R "$lproj" "$APP/Contents/Resources/"
done
cp "$ICON" "$APP/Contents/Resources/AppIcon.icns"
printf 'APPL????' >"$APP/Contents/PkgInfo"
cat >"$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>$APP_NAME</string>
	<key>CFBundleExecutable</key>
	<string>$EXECUTABLE</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundleIdentifier</key>
	<string>$BUNDLE_ID</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleLocalizations</key>
	<array>
		<string>en</string>
		<string>zh-Hans</string>
	</array>
	<key>CFBundleName</key>
	<string>$APP_NAME</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$SHORT_VERSION</string>
	<key>CFBundleSupportedPlatforms</key>
	<array>
		<string>MacOSX</string>
	</array>
	<key>CFBundleVersion</key>
	<string>$BUILD_NUMBER</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.education</string>
	<key>LSMinimumSystemVersion</key>
	<string>$MIN_MACOS</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSPrincipalClass</key>
	<string>NSApplication</string>
</dict>
</plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" >/dev/null || fail "Info.plist does not lint"

# ── 5. Sign (ad hoc, inside out) and verify ──────────────────────────────────────────────────
echo "==> codesign (ad hoc, hardened runtime)"
# The sidecar is its own main executable (AI apps launch it): identifier <bundle id>.cli.
codesign --force --sign - --options runtime --identifier "$BUNDLE_ID.cli" "$APP/Contents/MacOS/pagelamp"
codesign --force --sign - "$APP/Contents/Resources/PageLamp_PageLamp.bundle"
codesign --force --sign - --options runtime --identifier "$BUNDLE_ID" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP" 2>&1 | sed 's/^/    /'

# The SDK stamp decides AppKit's control metrics: it must be the real SDK (>= 26), and minos 26.0.
BUILD_INFO="$(vtool -show-build "$APP/Contents/MacOS/$EXECUTABLE")"
STAMPED_SDK="$(awk '$1 == "sdk" { print $2; exit }' <<<"$BUILD_INFO")"
STAMPED_MINOS="$(awk '$1 == "minos" { print $2; exit }' <<<"$BUILD_INFO")"
[[ -n "$STAMPED_SDK" ]] || fail "vtool shows no sdk for $EXECUTABLE"
(( ${STAMPED_SDK%%.*} >= 26 )) || fail "$EXECUTABLE is stamped sdk $STAMPED_SDK (< 26): pre-26 AppKit metrics"
# Without the -platform_version flags SwiftPM stamps sdk = minos (26.0), which passes the check
# above but hides the real SDK's behaviour; insist on the SDK we built with.
[[ "$STAMPED_SDK" == "$SDK_VERSION" ]] || fail "$EXECUTABLE is stamped sdk $STAMPED_SDK, built with $SDK_VERSION"
[[ "$STAMPED_MINOS" == "$MIN_MACOS" ]] || fail "$EXECUTABLE has minos $STAMPED_MINOS, expected $MIN_MACOS"
for binary in "$APP/Contents/MacOS/$EXECUTABLE" "$APP/Contents/MacOS/pagelamp"; do
  [[ "$(lipo -archs "$binary")" == "arm64" ]] || fail "$(basename "$binary") is not arm64-only"
done

echo "==> done in $(( $(seconds) - START )) s"
echo "    app:      ${APP#"$ROOT/"} ($(du -sh "$APP" | cut -f1))"
echo "    $EXECUTABLE: $(du -h "$APP/Contents/MacOS/$EXECUTABLE" | cut -f1), minos $STAMPED_MINOS, sdk $STAMPED_SDK"
echo "    pagelamp: $(du -h "$APP/Contents/MacOS/pagelamp" | cut -f1) (CLI sidecar)"
