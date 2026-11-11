#!/usr/bin/env bash
# scripts/probe/verify-starter-recipe.sh [--starter <tag>] [--keep]
#
# The starter recipe's probe track (RFC-0127 D5 guarantee 4, phase-551 W7).
# It builds the documented customer recipe (docker/starter-recipe/) FROM a
# starter image, then checks what the recipe relies on, with the network
# removed from every check:
#
#   guarantee 2 -- the baked store is the store that release's `nros setup`
#                  resolves: for every board the starter carries, and the one
#                  the recipe added, an OFFLINE `nros setup` finds every
#                  package already present. A real run, not `--dry-run`:
#                  before issue 2447 a dry run called present sources absent.
#   guarantee 3 -- NROS_HOME is /opt/nros, the working directory is /work,
#                  and the user is `nros`;
#   the recipe  -- its package is installed, and the book's first project
#                  still builds offline on top of it.
#
# Until RFC-0127 Q1 names a registry, <tag> is a locally built starter
# (scripts/ci/starter-image.sh). Nothing is pushed.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STARTER="nano-ros-starter:local"
KEEP=0
while [ $# -gt 0 ]; do
    case "$1" in
        --starter) STARTER="$2"; shift 2 ;;
        --keep) KEEP=1; shift ;;
        *) echo "usage: verify-starter-recipe.sh [--starter <tag>] [--keep]" >&2; exit 2 ;;
    esac
done
ENGINE="${NROS_IMAGE_ENGINE:-docker}"
RECIPE_DIR="$REPO_ROOT/docker/starter-recipe"
TAG="nano-ros-starter-recipe:probe"
NAME=verify-starter-recipe

"$ENGINE" image inspect "$STARTER" >/dev/null 2>&1 || {
    echo "$NAME: no starter image '$STARTER' here; build one first:" >&2
    echo "  scripts/ci/starter-image.sh --tag $STARTER" >&2
    exit 1
}
# The boards the starter baked are a property of the IMAGE (its label), not
# of this checkout's defaults, so a starter built with other --build-args is
# checked for what it actually carries.
boards="$("$ENGINE" image inspect --format '{{ index .Config.Labels "org.nano-ros.starter.boards" }}' "$STARTER")"
rmw="$("$ENGINE" image inspect --format '{{ index .Config.Labels "org.nano-ros.starter.rmw" }}' "$STARTER")"
if [ -z "$boards" ] || [ -z "$rmw" ]; then
    echo "$NAME: $STARTER carries no org.nano-ros.starter.{boards,rmw} labels -- not a starter image?" >&2
    exit 1
fi
team_board="$(sed -n 's/^ARG TEAM_BOARD=//p' "$RECIPE_DIR/Dockerfile")"

DOCKER_BUILDKIT=1 "$ENGINE" build --build-arg "STARTER=$STARTER" -t "$TAG" "$RECIPE_DIR"
[ "$KEEP" = 1 ] || trap '"$ENGINE" image rm -f "$TAG" >/dev/null 2>&1 || true' EXIT

fail=0
check() { # check <label> <script>
    if "$ENGINE" run --rm --network none -e NROS_OFFLINE=1 "$TAG" bash -c "$2"; then
        echo "$NAME: OK   $1"
    else
        echo "$NAME: FAIL $1" >&2
        fail=1
    fi
}

for b in $boards $team_board; do
    check "guarantee 2: $b is fully present offline" \
        "nros setup '$b' --rmw '$rmw' 2>&1 | tee /tmp/s.log >/dev/null; case \"\$(cat /tmp/s.log)\" in *'all packages already present'*) ;; *) tail -15 /tmp/s.log; exit 1 ;; esac"
done
# shellcheck disable=SC2016  # expanded inside the image, not here
check "guarantee 3: NROS_HOME, /work, user nros" \
    '[ "$NROS_HOME" = /opt/nros ] && [ "$(pwd)" = /work ] && [ "$(id -un)" = nros ]'
# The package check means something only if the STARTER lacks the package; a
# package the starter already ships passes it without the recipe's apt step.
if "$ENGINE" run --rm --network none "$STARTER" test -d /opt/ros/humble/share/nav_msgs; then
    echo "$NAME: FAIL $STARTER already ships nav_msgs, so the recipe's apt step proves nothing" >&2
    fail=1
fi
check "recipe: its package is installed" \
    'test -d /opt/ros/humble/share/nav_msgs'
check "recipe: the first project builds offline on top of it" \
    "cd /tmp && nros new p --board linux --template talker --lang rust --rmw '$rmw' >/dev/null && cd p && nros build >/tmp/b.log 2>&1 || { tail -15 /tmp/b.log; exit 1; }"

if [ "$fail" != 0 ]; then
    echo "$NAME: the recipe FROM $STARTER broke a guarantee (RFC-0127 D5)" >&2
    exit 1
fi
echo "$NAME: OK (recipe FROM $STARTER)"
