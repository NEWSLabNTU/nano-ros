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

baseline="${1:-${NROS_SUBMODULE_PIN_BASELINE:-origin/main}}"
local_ref="${2:-HEAD}"

# --- selftest, on the NORMAL path (phase-395) -------------------------------
#
# This gate spent its life able to report OK without comparing anything, and a
# negative control nobody runs would not have caught that. So it runs here, and
# it exercises the exact decision that was wrong: what happens when the baseline
# cannot be resolved. It re-invokes this same script, so the selftest cannot
# drift from the shipped logic.
#
# NROS_SUBMODULE_PINS_REENTRY is a RECURSION guard, not an opt-in flag — the
# inner runs must not selftest themselves. Named for what it does, because an
# earlier spelling with SELFTEST in the name read to both check-gate-selftests
# and to a human as "this only runs when asked", which is the thing being
# forbidden.
nros_submodule_pins_selftest() {
    [ -n "${NROS_SUBMODULE_PINS_REENTRY:-}" ] && return 0
    local missing="refs/heads/__nros_selftest_absent__" rc

    NROS_SUBMODULE_PINS_REENTRY=1 GITHUB_ACTIONS=true \
        bash "$0" "$missing" >/dev/null 2>&1
    rc=$?
    if [ "$rc" -ne 1 ]; then
        echo "submodule-pins SELFTEST FAILED (got rc=$rc, want 1): an unresolvable" >&2
        echo "  baseline must be FATAL in CI. That it was not is how this gate" >&2
        echo "  passed a real pin rewind at PR stage while comparing nothing." >&2
        exit 1
    fi

    NROS_SUBMODULE_PINS_REENTRY=1 GITHUB_ACTIONS= CI= \
        bash "$0" "$missing" >/dev/null 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "submodule-pins SELFTEST FAILED (got rc=$rc, want 0): a fresh LOCAL" >&2
        echo "  clone with no origin/main must still be able to run the fast tier." >&2
        exit 1
    fi
}
nros_submodule_pins_selftest

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root" || exit 2

if ! git rev-parse --verify --quiet "$baseline^{commit}" >/dev/null; then
    # A gate that cannot check must SAY SO, not report OK.
    #
    # `actions/checkout` is shallow by default, so on a pull request
    # `origin/main` usually does not exist — and this branch used to `exit 0`.
    # The gate therefore ran on every PR and compared NOTHING, for as long as it
    # had existed. Measured: a real play_launch pin rewind reached a pushed
    # branch and was caught only by a LOCAL run.
    #
    # gate.yml already documents this exact trap for a sibling check:
    # "the base ref is usually absent and the diff would fail — which the
    # fail-safe would turn into code=true forever, i.e. safe but never actually
    # firing". Same trap, different gate.
    #
    # Locally the skip is still right: a fresh clone genuinely has nothing to
    # compare against, and refusing to run the fast tier there helps nobody.
    if [ -n "${GITHUB_ACTIONS:-}${CI:-}" ]; then
        echo "submodule-pins: FAILED — baseline '$baseline' does not resolve, in CI." >&2
        echo "" >&2
        echo "  This gate is NETWORK-FREE by the check-fast contract, so it cannot" >&2
        echo "  fetch the base itself. The workflow must provide it before" >&2
        echo "  \`just check fast\`, e.g.:" >&2
        echo "" >&2
        echo "    git fetch --no-tags --depth=1 origin \\" >&2
        echo "      +refs/heads/\${GITHUB_BASE_REF:-main}:refs/remotes/origin/\${GITHUB_BASE_REF:-main}" >&2
        echo "" >&2
        echo "  or pass one via \$NROS_SUBMODULE_PIN_BASELINE. Failing instead of" >&2
        echo "  skipping is deliberate: a silent skip here is indistinguishable" >&2
        echo "  from a green check." >&2
        exit 1
    fi
    echo "submodule-pins: NOT CHECKED — baseline '$baseline' does not resolve." >&2
    echo "  (a fresh LOCAL clone with no origin/main yet; nothing to compare" >&2
    echo "   against. In CI this is a FAILURE, not a skip.)" >&2
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
            echo "    after a fetch." >&2
            echo "" >&2
            echo "    Two different causes, and they need different fixes:" >&2
            echo "" >&2
            echo "    1. The commit was never pushed. A pin nobody can resolve" >&2
            echo "       clones as a broken tree; push it FIRST, then bump the" >&2
            echo "       pointer. \`check submodule-commits-reachable\` is the gate" >&2
            echo "       that asks the remote and will also catch this." >&2
            echo "" >&2
            echo "    2. The submodule is a SHALLOW clone, so the baseline commit" >&2
            echo "       (the one before the tip) is simply absent. This is the" >&2
            echo "       usual case in CI, and it is not your pin's fault. The" >&2
            echo "       fetch above cannot help: \`git fetch --all\` does not" >&2
            echo "       deepen a shallow clone. check-fast is network-free by" >&2
            echo "       contract, so the WORKFLOW must provide the objects --" >&2
            echo "       see the 'Fetch submodule history' step in gate.yml." >&2
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
