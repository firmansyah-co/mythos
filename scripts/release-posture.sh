#!/usr/bin/env bash
# Decide whether this run may create a GitHub Release.
#
# Two gates, and BOTH are structural rather than stylistic:
#
#   1. The tag must carry no prerelease identifier. v1.2.3-dev.4 and v1.2.3-rc.2 are builds of
#      1.2.3, not releases; publishing one presents a dev build as shipped.
#
#   2. The tag must ALREADY EXIST. A release must never create its own tag as a side effect.
#      GitHub's create-release API creates a missing tag from `target_commitish`, which defaults
#      to the repository's default branch - so "create a release" on a branch push silently
#      tags whatever dev happens to point at. The hyphen guard does not cover this route: a
#      release-branch push produces v1.2.3 with no hyphen at all.
#
# Usage: release-posture.sh <tag> <tag_exists:true|false>
# Writes create / prerelease / make_latest / reason to GITHUB_OUTPUT when set.
set -euo pipefail

TAG="${1:?usage: release-posture.sh <tag> <tag_exists>}"
TAG_EXISTS="${2:?usage: release-posture.sh <tag> <tag_exists>}"

CREATE=true; PRERELEASE=false; MAKE_LATEST=true
REASONS=()

if [ "${TAG#*-}" != "$TAG" ]; then
  CREATE=false; PRERELEASE=true; MAKE_LATEST=false
  REASONS+=("${TAG} carries a prerelease identifier, so it is a build of the base version, not a release")
fi

if [ "$TAG_EXISTS" != "true" ]; then
  CREATE=false; PRERELEASE=false; MAKE_LATEST=false
  REASONS+=("${TAG} does not exist in this checkout, and a release must never create its own tag (GitHub would create it from target_commitish, which defaults to the default branch)")
fi

if [ "$CREATE" = true ]; then
  REASON="${TAG} is an existing final release tag"
  DETAIL="Creating the release for a tag that already exists (prerelease=false, make_latest=true)."
else
  REASON="$(printf '%s; ' "${REASONS[@]}")"; REASON="${REASON%; }"
  DETAIL="Every reason above blocks the release; none of them is a transient failure."
fi

echo "RELEASE: $( [ "$CREATE" = true ] && echo CREATE || echo NONE ). ${REASON}."
echo "${DETAIL}"
if [ "$CREATE" != true ]; then
  echo "(This is a deliberate skip, not the release step failing to run.)"
fi

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  {
    echo "create=${CREATE}"
    echo "prerelease=${PRERELEASE}"
    echo "make_latest=${MAKE_LATEST}"
    echo "reason=${REASON}"
  } >> "$GITHUB_OUTPUT"
fi
