#!/usr/bin/env bash
# Which OTHER nano-ros checkout did this environment come from? — issue 1280.
#
# Prints that checkout's root on stdout, or nothing when the environment names
# none. `just/sdk-env.just` calls this ONCE per `just` invocation, for the
# stderr ADVISORY below — the line that tells a reader their paths came from
# another checkout at all.
#
# It used to be load-bearing: 1280 rewrote every inherited SDK path by LEXICAL
# PREFIX, with this script's answer as the prefix. Issue 1391 measured why that
# could not be the rule. An agent worktree lives at
# `<main>/.claude/worktrees/<id>`, so the two checkouts NEST, the parent's root
# is a strict prefix of the worktree's, and the rewrite fired on values that
# were already correct — `<worktree>/<worktree-rel>/packages/...`, a path that
# does not exist, reported as a missing source file. Under nesting only the
# DEEPEST owning checkout separates "keep" from "re-root", which is a per-VALUE
# question and no prefix can answer it. `just/sdk-env.just` now puts each value
# through `nros_reroot_checkout_path` (via `reroot-checkout-path.sh`) instead.
#
# A note this script is NOT wrong about, since 1391 asked: a parent checkout
# IS foreign to a worktree nested inside it. An inherited value naming the
# parent must still be re-rooted — that is exactly 1280 — and a value naming
# the worktree is already skipped below, because its owner resolves to `here`.
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
    # Two or more foreign checkouts in one environment. Under 1280's single
    # prefix rewrite this had to REFUSE, because one prefix cannot be right for
    # both. Per-VALUE re-rooting has no such ambiguity — each value is judged
    # against its own owning checkout — so this is now REPORTED, not refused
    # (issue 1391). Still worth saying out loud: an environment assembled from
    # several checkouts is rarely what anyone meant.
    if [ -z "${NROS_QUIET_ACTIVATE:-}" ]; then
        {
            echo "nano-ros: this environment names MORE THAN ONE other nano-ros checkout."
            echo "  Inherited paths are re-rooted onto this one, each by its own owner."
            echo "    building here: $here_real"
            echo "    inherited from: $found"
            for other in $(printf '%s' "$extra" | tr ':' ' '); do
                echo "                    $other"
            done
            echo "  Start a clean shell in this checkout and \`source ./activate.sh\` (issue 1280)."
        } >&2
    fi
    printf '%s' "$found"
    exit 0
fi

if [ -z "${NROS_QUIET_ACTIVATE:-}" ]; then
    echo "nano-ros: re-rooting inherited paths from $found onto $here_real" >&2
    echo "  (a linked worktree inherits the parent shell's absolute SDK paths — issue 1280;" >&2
    echo "   \`source ./activate.sh\` here, or set NROS_QUIET_ACTIVATE=1 to silence this)." >&2
fi

printf '%s' "$found"
