#!/usr/bin/env bash
# scripts/ci/starter-latest.sh [--dry-run]
#
# RFC-0127 D7 (phase-551 W5): move `ghcr.io/newslabntu/nano-ros-starter:latest`
# to the newest released starter, and ONLY after the `image` probe track
# passes on it. Run by host-tests.yml (image-track); never by hand from a session.
#
#   1. find the newest `nros-<ver>` release whose `<ver>` image exists. A
#      release whose image job failed has no image; the one before it is then
#      the newest that can be probed, and `latest` stays where it was until
#      the failure is fixed. No published image at all is nothing in scope
#      (the first release with the image job has not run), and says so;
#   2. an anonymous pull of it is allowed (a private package is red: issue
#      2459, GHCR creates one private on its first push);
#   3. `just probe image <ref> pull=1`: a timed pull, the size budget, the
#      book's newcomer steps offline, the course recipe. Red stops here;
#   4. point `latest` at that version unless it already does.
#
# `--dry-run` does 1 to 3 and prints what 4 would do.
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

# A student pulls with no credentials, and this job is logged in, so its own
# pull proves nothing about theirs. GHCR creates a package PRIVATE on its
# first push (issue 2459): ask the registry as nobody. The image exists (the
# logged-in lookup above found it), so a refusal here means private, and
# `latest` must not move to an image a seat cannot pull.
anon_status() { # <owner/name> <tag> -> the manifest's HTTP status, anonymously
    local tok
    tok="$(curl -fsS "https://${1%%/*}/token?scope=repository:${1#*/}:pull" 2>/dev/null \
        | python3 -c 'import json, sys; print(json.load(sys.stdin)["token"])' 2>/dev/null)" \
        || { echo denied; return 0; }
    curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $tok" \
        -H 'Accept: application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json, application/vnd.docker.distribution.manifest.list.v2+json' \
        "https://${1%%/*}/v2/${1#*/}/manifests/$2"
}
status="$(anon_status "$IMAGE" "$version")"
if [ "$status" != 200 ]; then
    echo "starter-latest: $ref exists but an anonymous pull is refused ($status):" >&2
    echo "  the package is private. An org admin sets it public once, at" >&2
    echo "  https://github.com/orgs/newslabntu/packages/container/nano-ros-starter/settings" >&2
    echo "  (Danger Zone > Change visibility > Public); then re-run host-tests.yml (image-track)." >&2
    exit 1
fi
echo "starter-latest: $ref is pullable with no credentials"
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
