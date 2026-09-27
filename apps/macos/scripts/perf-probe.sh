#!/bin/zsh
# Measure UI jank on a release build with the in-app performance probe (Support/PerfProbe.swift).
#
#   scripts/perf-probe.sh [inspector|sidebar|navigate|courses|enter|capsule|segment|all] [repeats]
#
# courses = course to course, enter = This Week to a course, capsule = the sidebar's selection
# capsule driven by real clicks and keys (not part of all; its JSON judges every change). Opens a
# window for up to a minute (don't touch it; an inactive app may be capped at 60 fps) and prints
# one JSON line: per scenario the median first-frame delay, longest frame and hitch time per
# second (Apple: < 5 ms/s good, > 10 ms/s bad), the main thread's CPU time, and where the long
# frames were.
set -euo pipefail
cd "${0:A:h}/.."
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
sdk="$(xcrun --sdk macosx --show-sdk-version)"
swift build -c release --product PageLampApp \
  -Xlinker -platform_version -Xlinker macos -Xlinker 26.0 -Xlinker "$sdk" >/dev/null
PAGELAMP_PERF_PROBE="${1:-all}" PAGELAMP_PERF_TOGGLES="${2:-8}" \
  .build/out/Products/Release/PageLampApp 2>/dev/null | sed -n 's/^PAGELAMP_PERF //p'
