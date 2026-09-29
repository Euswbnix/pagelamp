#!/usr/bin/env bash
# Moves the updater channels on the gh-pages branch, for .github/workflows/channels.yml.
#
#   update-channels.sh release        TAG and PRERELEASE of a release that was just published:
#                                     updates/beta.json, and updates/stable.json unless it is a
#                                     pre-release, become its latest.json
#   update-channels.sh promote        TAG and CHANNEL: point one channel at a published release's
#                                     latest.json (the rollback of the "bad release" runbook)
#   update-channels.sh test-publish   RUN_ID of a rehearsal of release.yml from main: its
#                                     updater-test artifact becomes updates/test.json and
#                                     updates/test/<commit>/ (the test channel)
#   update-channels.sh test-delete    remove updates/test.json and updates/test/
#
# GitHub Pages serves the branch gh-pages from its root (the owner's one-time setup,
# docs/release-runbook.md), at $PAGES_URL. gh-pages holds only what Pages serves (.nojekyll,
# updates/) and is replaced by one new commit each time (a force push with a lease on the commit
# it started from), so the test channel's installers (tens of MB) never stay in the repository's
# history. Every change is in this workflow's run log. A manifest is checked
# (updater-manifest.mjs check) before any channel serves it, and the test channel's signatures
# are checked against the committed updater public key.
#
# Needs GH_TOKEN (contents: write; actions: read for test-publish), GH_REPO, PAGES_URL and
# RUNNER_TEMP. The token reaches git only as an HTTP header on the command line of the clone and
# the push; nothing is written to disk.
set -euo pipefail

: "${GH_TOKEN:?GH_TOKEN is not set}" "${GH_REPO:?GH_REPO is not set}" "${PAGES_URL:?PAGES_URL is not set}"
BRANCH=gh-pages
WORK="${RUNNER_TEMP:?RUNNER_TEMP is not set}/update-channels"
SITE="$WORK/site"
MANIFEST="$(cd "$(dirname "$0")" && pwd)/updater-manifest.mjs"
# CHANNELS_REMOTE is for testing this script against a local repository (it gets no token).
REMOTE="${CHANNELS_REMOTE:-https://github.com/$GH_REPO.git}"
# What a rehearsal serves on the test channel (the AppImage is too big for Pages).
TEST_PLATFORMS=darwin-aarch64-app,darwin-x86_64-app,windows-x86_64-nsis

fail() {
  echo "::error::$*"
  exit 1
}

summary() {
  echo "$*"
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then echo "- $*" >> "$GITHUB_STEP_SUMMARY"; fi
}

BASIC_AUTH=$(printf 'x-access-token:%s' "$GH_TOKEN" | base64 | tr -d '\n')
echo "::add-mask::$BASIC_AUTH"
git_auth() {
  git -c "http.https://github.com/.extraheader=AUTHORIZATION: basic $BASIC_AUTH" "$@"
}

# The current gh-pages files into $SITE (none if the branch doesn't exist yet), plus .nojekyll.
PREVIOUS_COMMIT=""
PREVIOUS_TREE=""
fetch_site() {
  local code=0
  mkdir -p "$SITE"
  git_auth ls-remote --exit-code --heads "$REMOTE" "refs/heads/$BRANCH" > "$WORK/ls-remote" || code=$?
  case "$code" in
    0)
      git_auth clone --quiet --depth 1 --branch "$BRANCH" "$REMOTE" "$WORK/current"
      git -C "$WORK/current" archive --format=tar HEAD | tar -x -C "$SITE"
      PREVIOUS_COMMIT=$(git -C "$WORK/current" rev-parse HEAD)
      PREVIOUS_TREE=$(git -C "$WORK/current" rev-parse 'HEAD^{tree}')
      ;;
    2) echo "There is no $BRANCH branch yet; this run creates it." ;;
    *) fail "Can't read the branches of $REMOTE (git ls-remote exit $code)" ;;
  esac
  touch "$SITE/.nojekyll"
  mkdir -p "$SITE/updates"
}

# Replaces gh-pages with one commit holding $SITE, unless nothing changed. The lease makes the
# push fail if gh-pages moved since fetch_site (the workflow's concurrency group should prevent
# that anyway).
push_site() {
  local message=$1 tree
  git -C "$SITE" init --quiet -b "$BRANCH"
  git -C "$SITE" add -A
  tree=$(git -C "$SITE" write-tree)
  if [ "$tree" = "$PREVIOUS_TREE" ]; then
    summary "gh-pages already serves exactly these files; nothing to change."
    return 0
  fi
  git -C "$SITE" -c user.name='github-actions[bot]' -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
    commit --quiet -m "$message" -m "Run: ${GITHUB_SERVER_URL:-https://github.com}/$GH_REPO/actions/runs/${GITHUB_RUN_ID:-local}"
  git_auth -C "$SITE" push --quiet --force-with-lease="refs/heads/$BRANCH:$PREVIOUS_COMMIT" "$REMOTE" "HEAD:refs/heads/$BRANCH"
  summary "gh-pages: $message"
  (cd "$SITE" && find . -path ./.git -prune -o -type f -print | sed 's|^\./||' | sort)
}

# A published release's latest.json into $WORK/release, checked against the contract and the
# release's assets. Returns 1 if the release has no latest.json (the updater was off for it).
download_manifest() {
  local tag=$1 dir="$WORK/release"
  mkdir -p "$dir"
  gh release view "$tag" --json assets --jq '.assets[].name' > "$dir/assets.txt" || fail "Can't read the release $tag"
  grep -qxF latest.json "$dir/assets.txt" || return 1
  gh release download "$tag" --pattern latest.json --dir "$dir" --clobber || fail "Can't download latest.json from $tag"
  node "$MANIFEST" check "$dir/latest.json" --tag "$tag" --repo "$GH_REPO" --assets "$dir/assets.txt" ||
    fail "latest.json of $tag doesn't follow the manifest contract (see above); no channel was changed"
}

cmd_release() {
  : "${TAG:?TAG is not set}" "${PRERELEASE:?PRERELEASE is not set}"
  local channels=beta channel
  [ "$PRERELEASE" = true ] || channels="beta stable"
  if ! download_manifest "$TAG"; then
    echo "::notice::$TAG has no latest.json (it was released with the updater off), so the update channels stay as they are."
    return 0
  fi
  fetch_site
  for channel in $channels; do
    cp "$WORK/release/latest.json" "$SITE/updates/$channel.json"
    summary "$channel → $TAG: $PAGES_URL/updates/$channel.json"
  done
  push_site "Update channels: ${channels// /, } → $TAG"
}

cmd_promote() {
  : "${TAG:?Enter the tag of the release the channel should serve}" "${CHANNEL:?CHANNEL is not set}"
  local state
  case "$CHANNEL" in beta | stable) ;; *) fail "Unknown channel '$CHANNEL' (beta or stable)" ;; esac
  state=$(gh release view "$TAG" --json isDraft,isPrerelease --jq '"\(.isDraft) \(.isPrerelease)"') || fail "There is no release $TAG"
  [ "${state% *}" = false ] || fail "$TAG is still a draft; its files can't be downloaded without signing in, so no channel may point at it"
  if [ "$CHANNEL" = stable ] && [ "${state#* }" = true ]; then
    fail "$TAG is a pre-release; the stable channel only serves full releases"
  fi
  download_manifest "$TAG" || fail "$TAG has no latest.json (it was released with the updater off)"
  fetch_site
  cp "$WORK/release/latest.json" "$SITE/updates/$CHANNEL.json"
  summary "$CHANNEL → $TAG (by ${GITHUB_ACTOR:-hand}): $PAGES_URL/updates/$CHANNEL.json"
  push_site "Update channels: $CHANNEL → $TAG (by hand)"
}

cmd_test_publish() {
  : "${RUN_ID:?Enter the ID of the rehearsal run}"
  local run path event branch status sha short name dir="$WORK/test" f
  case "$RUN_ID" in '' | *[!0-9]*) fail "The run ID is the number in the run's URL, not '$RUN_ID'" ;; esac
  run=$(gh api "repos/$GH_REPO/actions/runs/$RUN_ID" --jq '[.path, .event, .head_branch, .status, .head_sha] | @tsv') ||
    fail "There is no workflow run $RUN_ID in $GH_REPO"
  IFS=$'\t' read -r path event branch status sha <<< "$run"
  [ "$path" = .github/workflows/release.yml ] || fail "Run $RUN_ID is a run of $path, not of release.yml"
  [ "$event" = workflow_dispatch ] || fail "Run $RUN_ID was started by $event, so it isn't a rehearsal"
  [ "$branch" = main ] || fail "Run $RUN_ID built $branch; the test channel serves rehearsals of main"
  [ "$status" = completed ] || fail "Run $RUN_ID hasn't finished yet ($status)"
  name=$(gh api "repos/$GH_REPO/actions/runs/$RUN_ID/artifacts?per_page=100" \
    --jq '[.artifacts[] | select(.expired | not) | select(.name | startswith("updater-test-"))] | sort_by(.created_at) | last | .name // empty')
  [ -n "$name" ] || fail "Run $RUN_ID has no updater-test artifact: the updater was off, updater-manifest failed, or the artifact expired (7 days)"
  case "$name" in "updater-test-$sha-attempt"*) ;; *) fail "The artifact $name doesn't belong to the run's commit $sha" ;; esac
  short=${sha:0:7}
  gh run download "$RUN_ID" --name "$name" --dir "$dir"
  (cd "$dir" && ls) > "$WORK/test-assets.txt"
  node "$MANIFEST" check "$dir/latest.json" --base-url "$PAGES_URL/updates/test/$short/" \
    --platforms "$TEST_PLATFORMS" --assets "$WORK/test-assets.txt" ||
    fail "The test manifest of run $RUN_ID doesn't follow the contract (see above)"
  node "$MANIFEST" verify "$dir/latest.json" --dir "$dir" --pubkey-config apps/desktop/src-tauri/tauri.conf.json ||
    fail "The test channel's files don't match their signatures and the committed updater public key (see above)"
  fetch_site
  rm -rf "$SITE/updates/test"
  mkdir -p "$SITE/updates/test/$short"
  for f in "$dir"/*; do
    [ "$(basename "$f")" = latest.json ] || cp "$f" "$SITE/updates/test/$short/"
  done
  cp "$dir/latest.json" "$SITE/updates/test.json"
  summary "test → rehearsal $short (run $RUN_ID, version $(node -p 'require(process.argv[1]).version' "$dir/latest.json")): $PAGES_URL/updates/test.json"
  push_site "Test channel → rehearsal $short (run $RUN_ID)"
}

cmd_test_delete() {
  fetch_site
  rm -rf "$SITE/updates/test" "$SITE/updates/test.json"
  summary "test channel removed"
  push_site "Test channel removed"
}

rm -rf "$WORK"
mkdir -p "$WORK"
case "${1:-}" in
  release) cmd_release ;;
  promote) cmd_promote ;;
  test-publish) cmd_test_publish ;;
  test-delete) cmd_test_delete ;;
  *)
    echo "usage: $0 release|promote|test-publish|test-delete" >&2
    exit 2
    ;;
esac
