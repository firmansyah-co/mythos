#!/usr/bin/env bash
# Derive the release version from the repository's tags, or stamp it into the manifests.
#
# The manifests carry the BASE version (1.2.3), never the prerelease identifier: a tag like
# v1.2.3-rc.4 is a build OF 1.2.3, so stamping 1.2.3-rc.4 into package.json would make a
# released artifact's version depend on which build number happened to be pushed. The
# agreement test compares manifests to the tag's base, not to its full string.
#
# The tag is the record of what was released. The manifests are the reviewed intent. They are
# kept equal by crates/mythos-core/tests/version_agreement_tests.rs, which fails when they drift
# apart — that test is the reason this script can derive instead of trusting a hand-maintained
# constant (the previous form hardcoded BASE_VERSION="0.1.0", so the workflow would have
# published v0.1.0 forever).
#
# Usage:
#   scripts/version.sh --print            print BASE_VERSION / BUILD / TAG_NAME (+ GITHUB_OUTPUT)
#   scripts/version.sh --dry-run-stamp    print the stamping it WOULD apply, write nothing
#   scripts/version.sh --stamp            write the version into every manifest
#
# Context comes from the GitHub Actions environment when present:
#   GITHUB_EVENT_NAME  push | pull_request (anything else is treated as a local build)
#   GITHUB_REF         refs/heads/<branch> | refs/tags/<tag> | refs/pull/N/merge
#   GITHUB_REF_NAME    branch or tag name
#   GITHUB_BASE_REF    PR base branch
#   GITHUB_RUN_NUMBER  build counter used only when no tag exists yet
#   GITHUB_OUTPUT      file to append step outputs to
set -euo pipefail

MODE="${1:---print}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() { echo "version.sh: $*" >&2; exit 1; }

# --- derive -----------------------------------------------------------------

MANIFEST_VERSION="$(
  awk '
    /^\[workspace\.package\]/ { in_section = 1; next }
    /^\[/                     { in_section = 0 }
    in_section && /^version[[:space:]]*=/ {
      gsub(/.*=[[:space:]]*"|".*/, "", $0); print; exit
    }
  ' Cargo.toml
)"
[ -n "$MANIFEST_VERSION" ] || fail "could not read [workspace.package] version from Cargo.toml"

LAST_TAG="$(git describe --tags --abbrev=0 2>/dev/null || true)"

if [ -n "$LAST_TAG" ]; then
  TAG_VERSION="${LAST_TAG#v}"
  # Core version: strip any prerelease/build suffix. v0.2.0-rc.3 -> 0.2.0
  BASE_VERSION="${TAG_VERSION%%-*}"
  BASE_VERSION="${BASE_VERSION%%+*}"
  # Build counter: commits since the last release, NOT commits from the root of history.
  # It resets at each tag, which is the intended meaning ("builds since the last release").
  BUILD="$(git rev-list --count "${LAST_TAG}..HEAD")"
  BUILD_SOURCE="commits since ${LAST_TAG}"
else
  # Bootstrap before the first tag. There is no distance from a tag that does not exist, so
  # the counter starts at 0. Deliberately NOT the workflow run number: the first tag would
  # then be -dev.<N> and the very next dev build would measure 1 commit since it, so the
  # counter would DECREASE at its first boundary. 0 is the only value that keeps it monotonic
  # across the first tag - and it is why the first tag is v0.1.0-dev.0 rather than -dev.134.
  BASE_VERSION="$MANIFEST_VERSION"
  BUILD="0"
  BUILD_SOURCE="no tag yet: the counter starts at 0 so it never decreases at the first tag"
fi

EVENT_NAME="${GITHUB_EVENT_NAME:-local}"
BASE_REF="${GITHUB_BASE_REF:-}"
REF_NAME="${GITHUB_REF_NAME:-}"

case "$EVENT_NAME" in
  push)
    if [ "${GITHUB_REF:-}" != "${GITHUB_REF#refs/tags/v}" ]; then
      TAG_NAME="${GITHUB_REF#refs/tags/}"          # a tag push: the tag already names the version
    elif [ "$REF_NAME" = "release" ]; then
      TAG_NAME="v${BASE_VERSION}"                   # no prerelease identifier on release
    elif [ "$REF_NAME" = "rc" ]; then
      TAG_NAME="v${BASE_VERSION}-rc.${BUILD}"
    else
      TAG_NAME="v${BASE_VERSION}-dev.${BUILD}"
    fi
    ;;
  pull_request)
    case "$BASE_REF" in
      release) TAG_NAME="v${BASE_VERSION}" ;;
      rc)      TAG_NAME="v${BASE_VERSION}-rc.${BUILD}" ;;
      *)       TAG_NAME="v${BASE_VERSION}-dev.${BUILD}" ;;
    esac
    ;;
  *)
    TAG_NAME="v${BASE_VERSION}-dev.${BUILD}"
    ;;
esac

VERSION="${TAG_NAME#v}"

# --- stamping targets -------------------------------------------------------

# Every manifest that carries a version. Listed explicitly: a glob that silently matches
# nothing is the failure mode this repository keeps removing.
STAMP_FILES=(
  Cargo.toml
  frontend/package.json
  frontend/packages/ui-empty-state/package.json
  frontend/packages/ui-shell/package.json
  frontend/plugins/ui-faceplate-lib/package.json
  frontend/plugins/ui-inspector/package.json
  frontend/plugins/ui-logic-editor/package.json
  frontend/plugins/ui-topology/package.json
)

current_version_of() {
  case "$1" in
    Cargo.toml)
      awk '
        /^\[workspace\.package\]/ { in_section = 1; next }
        /^\[/                     { in_section = 0 }
        in_section && /^version[[:space:]]*=/ {
          gsub(/.*=[[:space:]]*"|".*/, "", $0); print; exit
        }
      ' "$1"
      ;;
    *)
      sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | head -1
      ;;
  esac
}

stamp_file() {
  local file="$1" target="$2" tmp
  case "$file" in
    Cargo.toml)
      tmp="$(mktemp)"
      awk -v v="$target" '
        /^\[workspace\.package\]/ { in_section = 1; print; next }
        /^\[/                     { in_section = 0 }
        in_section && !done && /^version[[:space:]]*=/ { print "version = \"" v "\""; done = 1; next }
        { print }
      ' "$file" > "$tmp"
      mv "$tmp" "$file"
      ;;
    *)
      tmp="$(mktemp)"
      awk -v v="$target" '
        !done && match($0, /^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"/) {
          sub(/"version"[[:space:]]*:[[:space:]]*"[^"]*"/, "\"version\": \"" v "\"")
          done = 1
        }
        { print }
      ' "$file" > "$tmp"
      mv "$tmp" "$file"
      ;;
  esac
}

# --- modes ------------------------------------------------------------------

case "$MODE" in
  --print)
    echo "Manifest version:  ${MANIFEST_VERSION}"
    echo "Last tag:          ${LAST_TAG:-<none>}"
    echo "Base version:      ${BASE_VERSION}"
    echo "Build:             ${BUILD} (${BUILD_SOURCE})"
    echo "Tag name:          ${TAG_NAME}"
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
      {
        echo "base_version=${BASE_VERSION}"
        echo "build=${BUILD}"
        echo "tag_name=${TAG_NAME}"
        echo "version=${VERSION}"
      } >> "$GITHUB_OUTPUT"
    fi
    ;;
  --dry-run-stamp|--stamp)
    echo "Would stamp version ${BASE_VERSION} into:"
    for f in "${STAMP_FILES[@]}"; do
      [ -f "$f" ] || fail "expected manifest missing: $f"
      old="$(current_version_of "$f")"
      [ -n "$old" ] || fail "no version field found in $f"
      printf '  %-52s %s -> %s\n' "$f" "$old" "$BASE_VERSION"
    done
    if [ "$MODE" = "--stamp" ]; then
      for f in "${STAMP_FILES[@]}"; do stamp_file "$f" "$BASE_VERSION"; done
      echo "Stamped ${#STAMP_FILES[@]} files with ${BASE_VERSION}."
    else
      echo "(dry run: nothing written)"
    fi
    ;;
  *)
    fail "unknown mode: $MODE (use --print, --dry-run-stamp or --stamp)"
    ;;
esac
