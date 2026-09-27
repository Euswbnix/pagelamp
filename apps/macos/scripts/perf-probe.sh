#!/bin/zsh
# Measure UI jank on a release build with the in-app performance probe (Support/PerfProbe.swift).
#
#   scripts/perf-probe.sh [inspector|sidebar|navigate|courses|enter|capsule|segment|all] [repeats]
#
# courses = course to course, enter = This Week to a course, capsule = the sidebar's selection
# capsule driven by real clicks and keys, segment = the course section picker's glass thumb on
# every kind of change (clicks with PAGELAMP_PERF_PRESS ms between down and up, both 50 and 0 when
# unset; keys, VoiceOver's press, the model, cross-links); neither is part of all, and their JSON
# judges every change (the app must be frontmost for clicks: see appActiveAtStart). Opens a
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
