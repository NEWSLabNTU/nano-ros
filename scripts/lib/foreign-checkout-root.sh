#!/usr/bin/env bash
# Which OTHER nano-ros checkout did this environment come from? — issue 1280.
#
# Prints that checkout's root on stdout, or nothing when the environment names
# none. `just/sdk-env.just` calls this ONCE per `just` invocation and uses the
# answer as a prefix to rewrite with: every inherited SDK path was rooted at
# one `justfile_directory()` in the shell that exported them, so one prefix
# rewrites all of them, and a value pointing OUTSIDE any checkout does not
# contain the prefix and is therefore left exactly as it was — which is the
# out-of-tree-SDK case env-first exists to serve.
#
# The scan is over the WHOLE environment rather than over a list of names. The
# issue's own census was 19 variables and was already short by five, and its
# 2026-09-12 addendum says why a list is the wrong shape here: the rule is
# about any absolute path inherited from an ancestor checkout, so a list can
# only ever be behind. An environment scan cannot go stale.
#
# usage: foreign-checkout-root.sh <here>

set -eu

here="${1:-}"
if [ -z "$here" ]; then
    echo "foreign-checkout-root.sh: <here> is required" >&2
    exit 2
fi

. "$(dirname "$0")/checkout-paths.sh"

here_real="$(cd "$here" 2>/dev/null && pwd -P)" || here_real=""
if [ -z "$here_real" ]; then
    # No usable "this checkout" — answer nothing rather than guess. Every
    # caller treats an empty answer as "no rewrite".
    exit 0
fi

# Values considered: a SINGLE absolute path. A `:`-separated list (PATH,
# LD_LIBRARY_PATH, AMENT_PREFIX_PATH) is deliberately skipped — it is not a
# path-valued build input, and one entry of another checkout's PATH would
# otherwise claim a foreign root for an environment that has none.
found=""
extra=""
while IFS= read -r name; do
    eval "value=\${$name-}"
    # shellcheck disable=SC2154  # assigned by the eval above
    case "$value" in
        /*) ;;
        *) continue ;;
    esac
    case "$value" in
        *:*) continue ;;
    esac
    owner="$(nros_checkout_root "$value")"
    [ -n "$owner" ] || continue
    owner_real="$(cd "$owner" 2>/dev/null && pwd -P)" || continue
    [ "$owner_real" != "$here_real" ] || continue
    if [ -z "$found" ]; then
        found="$owner_real"
    elif [ "$found" != "$owner_real" ]; then
        case ":$extra:" in
            *":$owner_real:"*) ;;
            *) extra="${extra:+$extra:}$owner_real" ;;
        esac
    fi
done <<EOF
$(env | sed -n 's/^\([A-Za-z_][A-Za-z0-9_]*\)=.*/\1/p' | sort -u)
EOF

if [ -z "$found" ]; then
    exit 0
fi

if [ -n "$extra" ]; then
    # Two or more foreign checkouts in one environment: a single prefix rewrite
    # cannot be right for both, so REFUSE naming every path involved rather
    # than silently picking one. `just` aborts on a failed `shell()` and shows
    # this text.
    {
        echo "nano-ros: this environment names MORE THAN ONE other nano-ros checkout,"
        echo "  so the inherited absolute paths cannot be re-rooted onto this one."
        echo "    building here: $here_real"
        echo "    inherited from: $found"
        for other in $(printf '%s' "$extra" | tr ':' ' '); do
            echo "                    $other"
        done
        echo "  Start a clean shell in this checkout and \`source ./activate.sh\` (issue 1280)."
    } >&2
    exit 1
fi

if [ -z "${NROS_QUIET_ACTIVATE:-}" ]; then
    echo "nano-ros: re-rooting inherited paths from $found onto $here_real" >&2
    echo "  (a linked worktree inherits the parent shell's absolute SDK paths — issue 1280;" >&2
    echo "   \`source ./activate.sh\` here, or set NROS_QUIET_ACTIVATE=1 to silence this)." >&2
fi

printf '%s' "$found"
