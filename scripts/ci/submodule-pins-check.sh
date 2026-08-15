#!/usr/bin/env bash
#
# A submodule pin may only move FORWARD.
#
# Every submodule here lives on `main` (or a dedicated branch) with linear
# history, so advancing a pin means "fast-forward to a descendant". Anything
# else — an older commit, or a commit on a different line — is either a mistake
# or a decision that needs saying out loud.
#
# WHY THIS EXISTS. On 2026-08-15 `f003d0cb1` bumped zenoh-pico to d3f0d268 with
# a message naming the fix ("Zephyr declares `socklen_t` as..."). Ninety minutes
# later `e56354410` — a 24-file commit about renumbering ISSUE IDS, whose
# message never mentions the submodule — moved the same pin back to 43ddb0ec.
# The Zephyr build fix was silently unshipped for seven hours, and nothing
# noticed until a rebase conflict surfaced it. That is the `git add -A` hazard
# CLAUDE.md already warns about, one layer down: the pointer is a FILE, a
# blanket add scoops it up, and a pointer diff looks like noise in a large
# commit.
#
# A backward move cannot be caught by reading a diff — `-Subproject commit
# d3f0d26 / +Subproject commit 43ddb0e` is two hex strings, and which one is
# newer is not visible without asking the submodule. So ask it.
#
# Usage:
#   scripts/ci/submodule-pins-check.sh [<baseline-ref> [<local-ref>]]
#     baseline defaults to origin/main, local to HEAD.
#   The pre-push hook passes the REMOTE's actual sha as the baseline, which is
#   more precise than origin/main (that ref can be stale).
#
# Cost is proportional to pins that MOVED: an unchanged pin needs no submodule
# and no network.
#
# Bypass for a deliberate rollback: NROS_ALLOW_SUBMODULE_REWIND=1, and say why
# in the commit message.

set -uo pipefail

baseline="${1:-origin/main}"
local_ref="${2:-HEAD}"

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root" || exit 2

if ! git rev-parse --verify --quiet "$baseline^{commit}" >/dev/null; then
    echo "submodule-pins: NOT CHECKED — baseline '$baseline' does not resolve." >&2
    echo "  (a fresh clone with no origin/main yet; nothing to compare against)" >&2
    exit 0
fi

# `<mode> <type> <sha>\t<path>` for every gitlink, at one commit.
pins_at() {
    git ls-tree -r "$1" 2>/dev/null | awk '$2 == "commit" { print $4 "\t" $3 }'
}

baseline_pins="$(pins_at "$baseline")"
local_pins="$(pins_at "$local_ref")"

fail=0
moved=0

while IFS=$'\t' read -r path new_sha; do
    [ -n "${path:-}" ] || continue
    old_sha="$(printf '%s\n' "$baseline_pins" | awk -F'\t' -v p="$path" '$1 == p { print $2 }')"

    # New submodule, or unchanged: nothing to prove.
    [ -z "$old_sha" ] && continue
    [ "$old_sha" = "$new_sha" ] && continue

    moved=$((moved + 1))

    if [ ! -e "$path/.git" ]; then
        echo "submodule-pins: CANNOT VERIFY $path" >&2
        echo "    the pin moved ${old_sha:0:12} -> ${new_sha:0:12} but the submodule is not" >&2
        echo "    initialised here, so its history cannot be read." >&2
        echo "    Run: git submodule update --init $path" >&2
        fail=1
        continue
    fi

    # Both commits must be present locally to compare them. A pin that moved
    # forward normally has them; fetch once if not (the sha may live only on the
    # remote when someone else advanced it).
    for sha in "$old_sha" "$new_sha"; do
        if ! git -C "$path" cat-file -e "${sha}^{commit}" 2>/dev/null; then
            git -C "$path" fetch --quiet --all 2>/dev/null || true
            break
        fi
    done

    for sha in "$old_sha" "$new_sha"; do
        if ! git -C "$path" cat-file -e "${sha}^{commit}" 2>/dev/null; then
            echo "submodule-pins: CANNOT VERIFY $path" >&2
            echo "    commit ${sha:0:12} is not in the submodule's object store, even" >&2
            echo "    after a fetch. A pin nobody can resolve clones as a broken tree." >&2
            echo "    Push the submodule commit FIRST, then bump the pointer." >&2
            fail=1
            continue 2
        fi
    done

    if git -C "$path" merge-base --is-ancestor "$old_sha" "$new_sha" 2>/dev/null; then
        continue  # fast-forward: the only sanctioned move
    fi

    # Not an ancestor. Say WHICH kind of wrong it is — a rewind and a fork need
    # different fixes, and the diff looks identical for both.
    if git -C "$path" merge-base --is-ancestor "$new_sha" "$old_sha" 2>/dev/null; then
        kind="REWIND — the new pin is an ANCESTOR of the old one"
        remedy="If you meant to keep the newer commit, restore it:
        git -C $path checkout $old_sha && git add $path"
    else
        kind="DIVERGED — neither pin contains the other"
        remedy="Rebase the submodule work onto its branch so the move is a
        fast-forward, then re-add the pointer. Merges are not used here."
    fi

    subject="$(git -C "$path" log -1 --format='%s' "$old_sha" 2>/dev/null)"
    echo "submodule-pins: $path" >&2
    echo "    $kind" >&2
    echo "      was: ${old_sha:0:12}  $subject" >&2
    echo "      now: ${new_sha:0:12}  $(git -C "$path" log -1 --format='%s' "$new_sha" 2>/dev/null)" >&2
    echo "    $remedy" >&2
    fail=1
done <<< "$local_pins"

if [ "$fail" -ne 0 ]; then
    if [ "${NROS_ALLOW_SUBMODULE_REWIND:-0}" = "1" ]; then
        echo "" >&2
        echo "submodule-pins: OVERRIDDEN by NROS_ALLOW_SUBMODULE_REWIND=1 — say why in" >&2
        echo "  the commit message, or the next reader will assume it was an accident." >&2
        exit 0
    fi
    echo "" >&2
    echo "  A pin moving backward silently unships whatever the skipped commits fixed." >&2
    echo "  Deliberate rollback: NROS_ALLOW_SUBMODULE_REWIND=1 (and say why)." >&2
    exit 1
fi

echo "submodule-pins: OK ($moved pin(s) moved, all fast-forward)"
exit 0
