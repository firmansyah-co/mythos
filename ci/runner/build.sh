#!/usr/bin/env bash
# Build the runner image from this directory and put it in the registry the cluster pulls from.
# Run by hand after changing Dockerfile or packages.txt, and weekly by the timer (README.md).
# It publishes nothing outside this machine: the registry listens on 127.0.0.1 only.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
IMAGE="${MYTHOS_RUNNER_IMAGE:-localhost:5000/mythos-arc-runner}"

podman build --pull=always -t "$IMAGE:latest" "$HERE"

# Never publish an image that would make every job fall back to apt, or one that cannot run.
PACKAGES="$(tr '\n' ' ' <"$HERE/packages.txt")"
podman run --rm --entrypoint /bin/bash "$IMAGE:latest" -c "dpkg -s $PACKAGES >/dev/null"
# The runner prints log lines around its version; the version is the line that is only a version.
VERSION="$(podman run --rm --entrypoint /home/runner/bin/Runner.Listener "$IMAGE:latest" --version 2>/dev/null |
    grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' | sed -n 1p || true)"
case "$VERSION" in
[0-9]*.[0-9]*.[0-9]*) ;;
*) echo "the image did not report a runner version: '$VERSION'" >&2; exit 1 ;;
esac

# `latest` is what the scale set pulls; the versioned tag is what to go back to.
podman push --tls-verify=false "$IMAGE:latest"
podman push --tls-verify=false "$IMAGE:latest" "$IMAGE:runner-$VERSION"
podman image prune -f >/dev/null
echo "published $IMAGE:latest and $IMAGE:runner-$VERSION (runner $VERSION, packages present)"
