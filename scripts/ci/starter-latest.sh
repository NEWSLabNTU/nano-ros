#!/usr/bin/env bash
# scripts/ci/starter-latest.sh [--dry-run]
#
# RFC-0127 D7 (phase-551 W5): move `ghcr.io/newslabntu/nano-ros-starter:latest`
# to the newest released starter, and ONLY after the `image` probe track
# passes on it. Run by probe.yml; never by hand from a session.
#
#   1. find the newest `nros-<ver>` release whose `<ver>` image exists. A
#      release whose image job failed has no image; the one before it is then
#      the newest that can be probed, and `latest` stays where it was until
#      the failure is fixed. No published image at all is nothing in scope
#      (the first release with the image job has not run), and says so;
#   2. `just probe image <ref> pull=1`: a timed pull, the size budget, the
#      book's newcomer steps offline, the course recipe. Red stops here;
#   3. point `latest` at that version unless it already does.
#
# `--dry-run` does 1 and 2 and prints what 3 would do.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="${NROS_STARTER_IMAGE_REPO:-ghcr.io/newslabntu/nano-ros-starter}"
DRY=0
[ "${1:-}" = --dry-run ] && DRY=1

version=""
if ! tags="$(gh release list --limit 30 --json tagName,createdAt \
        --jq 'sort_by(.createdAt) | reverse | .[].tagName')"; then
    echo "starter-latest: cannot list releases" >&2
    exit 1
fi
for t in $tags; do
    case "$t" in nros-*) ;; *) continue ;; esac
    v="${t#nros-}"
    if docker manifest inspect "$IMAGE:$v" >/dev/null 2>&1; then
        version="$v"
        break
    fi
    echo "starter-latest: release $t has no image $IMAGE:$v"
done
if [ -z "$version" ]; then
    echo "starter-latest: no release has a published starter image yet; nothing to probe"
    exit 0
fi

ref="$IMAGE:$version"
echo "starter-latest: probing $ref"
"$REPO_ROOT/scripts/probe/verify-starter-image.sh" "$ref" --pull

want="$(docker buildx imagetools inspect --format '{{json .Manifest.Digest}}' "$ref" | tr -d '"')"
have="$(docker buildx imagetools inspect --format '{{json .Manifest.Digest}}' "$IMAGE:latest" 2>/dev/null | tr -d '"' || true)"
if [ "$want" = "$have" ]; then
    echo "starter-latest: $IMAGE:latest is already $version ($want)"
    exit 0
fi
if [ "$DRY" = 1 ]; then
    echo "starter-latest: would move $IMAGE:latest ${have:-(unset)} -> $version ($want)"
    exit 0
fi
docker buildx imagetools create -t "$IMAGE:latest" "$ref"
echo "starter-latest: moved $IMAGE:latest -> $version ($want)"
