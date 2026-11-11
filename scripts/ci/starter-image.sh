#!/usr/bin/env bash
# scripts/ci/starter-image.sh [--target starter|starter-zephyr] [--tag <tag>]
#                             [-- <docker build args>...]
#
# Build a starter image LOCALLY (RFC-0127 D2/D3, phase-551 W2) and print its
# size against the RFC-0127 D7 budget: 2 GB compressed for `starter`, 5 GB for
# `starter-zephyr`. It never pushes and never tags anything outside this
# engine. Publishing waits on RFC-0127 Q1 and Q5.
#
# The context is written by `image-context.sh starter` into a temp dir. The
# image's own last step rebuilds the first projects with networking off, so a
# build that finishes has already shown the offline claim holds.
#
# Example, pinning the release the image installs:
#   scripts/ci/starter-image.sh -- --build-arg NROS_VERSION=0.6.0-nros8
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET=starter
TAG=""
while [ $# -gt 0 ]; do
    case "$1" in
        --target) TARGET="$2"; shift 2 ;;
        --tag) TAG="$2"; shift 2 ;;
        --) shift; break ;;
        *) echo "usage: starter-image.sh [--target starter|starter-zephyr] [--tag <tag>] [-- <docker build args>...]" >&2
           exit 2 ;;
    esac
done
case "$TARGET" in
    starter) budget=$((2 * 1000 * 1000 * 1000)) ;;
    starter-zephyr) budget=$((5 * 1000 * 1000 * 1000)) ;;
    *) echo "starter-image.sh: unknown target '$TARGET' (starter, starter-zephyr)" >&2; exit 2 ;;
esac
TAG="${TAG:-nano-ros-$TARGET:local}"
ENGINE="${NROS_IMAGE_ENGINE:-docker}"

ctx="$(mktemp -d)"
trap 'rm -rf "$ctx"' EXIT
bash "$REPO_ROOT/scripts/ci/image-context.sh" starter "$ctx/context" >/dev/null

DOCKER_BUILDKIT=1 "$ENGINE" build --target "$TARGET" -t "$TAG" "$@" "$ctx/context"

raw="$("$ENGINE" image inspect --format '{{.Size}}' "$TAG")"
compressed="$("$ENGINE" save "$TAG" | gzip -6 | wc -c)"
gb() { awk -v b="$1" 'BEGIN { printf "%.2f GB", b / 1e9 }'; }
echo "starter-image: $TAG -- $(gb "$raw") raw, $(gb "$compressed") compressed (gzip -6); budget $(gb "$budget")"
if [ "$compressed" -gt "$budget" ]; then
    echo "starter-image: $TARGET is over its compressed budget (RFC-0127 D7)" >&2
    exit 1
fi
