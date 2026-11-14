#!/usr/bin/env bash
# Diagnostic-code guard (RFC-0129 D3, phase-552 W1): a code is CLAIMED on
# origin or it is not allocated, and a number never changes its name.
#
#   scripts/ci/diag-codes-check.sh [<remote>]       the pre-push check
#   scripts/ci/diag-codes-check.sh --no-remote      (a) only, no network
#   scripts/ci/diag-codes-check.sh --self-test      planted defects
#
# The branch's codes.lock (the working tree's, as `issue-ids-check.sh` reads
# the working tree's issue files) is refused when
#
#   (a) it gives a number that main's lock (the local `origin/main` ref) has
#       under a DIFFERENT name: two sessions wrote the same number, and the
#       later one has to take a new claim. No `origin/main`, or no lock on
#       it, is NOT VERIFIED (said, not refused);
#   (b) it adds a number main's lock does not have and the remote holds no
#       `refs/diag-codes/NNNN` claim for it: the number was picked by hand,
#       not claimed with `just code-new`. The founding range written in the
#       lock's `# founding: NROS-0001..NROS-NNNN` line has no claims and is
#       exempt. An unreachable remote is a warning, not a refusal: the CI
#       gate `check-diagnostics` still holds (a), and a push that cannot reach
#       the remote does not land anyway.
#
# The pre-push hook runs this for BRANCH pushes only. A push of
# `refs/diag-codes/*` is how a number is claimed; guarding it would deadlock
# the tool that resolves a collision behind the collision (the lesson the
# issue-id guard learnt first).
#
# Exit: 0 clean (or not verified), 1 refused, 2 usage.
set -euo pipefail

cd "$(dirname "$0")/../.."
# The self-test builds throwaway repositories; an inherited GIT_DIR must not
# point them at this checkout (issue 0986).
# shellcheck source=scripts/lib/git-hook-env.sh
. scripts/lib/git-hook-env.sh
nros_clear_inherited_git_env
# shellcheck source=scripts/lib/git-history.sh
. scripts/lib/git-history.sh

LOCK_REL="packages/tooling/nros-diagnostic-codes/codes.lock"
REF_NS="refs/diag-codes"

# lock_rows <text> — "NNNN name" per row, comments skipped.
lock_rows() {
    local line
    while IFS= read -r line; do
        [[ "$line" =~ ^NROS-([0-9]{4})[[:space:]]+([A-Za-z0-9_]+)[[:space:]] ]] || continue
        printf '%s %s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}"
    done <<<"$1"
}

# founding_max <text> — the founding range's last number, 0 when none.
founding_max() {
    local line
    while IFS= read -r line; do
        if [[ "$line" =~ ^#[[:space:]]*founding:[[:space:]]*NROS-0001\.\.NROS-([0-9]{4})[[:space:]]*$ ]]; then
            printf '%d' "$((10#${BASH_REMATCH[1]}))"
            return
        fi
    done <<<"$1"
    printf '0'
}

# judge <branch-lock> <main-lock or empty> <claims or "-" when unknown>
# Prints one line per problem; rc 1 when there is one. `claims` is the
# newline-separated NNNN list of claim refs, or "-" (remote not asked).
judge() {
    local branch="$1" main="$2" claims="$3" num name main_name fmax bad=0 c
    declare -A have=() claimed=()
    while read -r num name; do
        [ -n "$num" ] && have[$num]="$name"
    done <<<"$(lock_rows "$main")"
    if [ "$claims" != "-" ]; then
        while read -r c; do
            [ -n "$c" ] && claimed[$c]=1
        done <<<"$claims"
    fi
    fmax="$(founding_max "$branch")"
    while read -r num name; do
        [ -n "$num" ] || continue
        main_name="${have[$num]:-}"
        if [ -n "$main_name" ]; then
            if [ "$main_name" != "$name" ]; then
                echo "NROS-$num is \`$main_name\` on main and \`$name\` here: a number never changes its name. Claim a new one: just code-new $name"
                bad=1
            fi
            continue
        fi
        [ "$((10#$num))" -le "$fmax" ] && continue
        [ "$claims" = "-" ] && continue
        if [ -z "${claimed[$num]:-}" ]; then
            echo "NROS-$num ($name) is new here and has no claim ($REF_NS/$num) on the remote: a code is claimed with \`just code-new $name\`, never picked by hand"
            bad=1
        fi
    done <<<"$(lock_rows "$branch")"
    return "$bad"
}

self_test() {
    local tmp out rc
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    local hdr='# founding: NROS-0001..NROS-0002'
    local main_lock="$hdr
NROS-0001 A live
NROS-0002 B live
NROS-0003 C live"
    # clean: main's rows, plus a claimed new one.
    judge "$main_lock
NROS-0004 D live" "$main_lock" $'0003\n0004' >/dev/null \
        || { echo "self-test: a claimed new code was refused" >&2; return 1; }
    # (a) a clash: main's number under another name.
    rc=0; out="$(judge "$hdr
NROS-0001 A live
NROS-0002 B live
NROS-0003 Mine live" "$main_lock" $'0003')" || rc=$?
    [ "$rc" -eq 1 ] && [[ "$out" == *"never changes its name"* ]] \
        || { echo "self-test: a clash passed: $out" >&2; return 1; }
    # (b) an unclaimed new number.
    rc=0; out="$(judge "$main_lock
NROS-0005 E live" "$main_lock" $'0003')" || rc=$?
    [ "$rc" -eq 1 ] && [[ "$out" == *"has no claim"* ]] \
        || { echo "self-test: an unclaimed number passed: $out" >&2; return 1; }
    # the founding range needs no claim, even with no main lock at all.
    judge "$hdr
NROS-0001 A live
NROS-0002 B live" "" "" >/dev/null \
        || { echo "self-test: the founding range was refused" >&2; return 1; }
    rc=0; judge "$main_lock" "" "" >/dev/null || rc=$?
    # ...but 0003 is outside it, so with no main lock and no claim it is refused.
    [ "$rc" -eq 1 ] || { echo "self-test: 0003 passed outside the founding range" >&2; return 1; }
    # a remote that was not asked never refuses (b).
    judge "$main_lock
NROS-0005 E live" "$main_lock" "-" >/dev/null \
        || { echo "self-test: an unasked remote refused" >&2; return 1; }

    # End to end against scratch repositories: the claim listing comes from a
    # real bare remote, and main's lock from a real `origin/main`.
    git init -q --bare "$tmp/remote.git"
    git init -q "$tmp/work"
    mkdir -p "$tmp/work/$(dirname "$LOCK_REL")"
    printf '%s\n' "$main_lock" >"$tmp/work/$LOCK_REL"
    git -C "$tmp/work" add "$LOCK_REL"
    git -C "$tmp/work" -c user.name=t -c user.email=t@t commit -q -m main
    git -C "$tmp/work" push -q "$tmp/remote.git" HEAD:refs/heads/main
    git -C "$tmp/work" remote add origin "$tmp/remote.git"
    git -C "$tmp/work" fetch -q origin
    git -C "$tmp/work" push -q origin "HEAD:$REF_NS/0003" "HEAD:$REF_NS/0004"
    printf '%s\nNROS-0004 D live\n' "$main_lock" >"$tmp/work/$LOCK_REL"
    (cd "$tmp/work" && check origin) >/dev/null \
        || { echo "self-test: end to end, a claimed code was refused" >&2; return 1; }
    printf '%s\nNROS-0004 D live\nNROS-0005 E live\n' "$main_lock" >"$tmp/work/$LOCK_REL"
    rc=0; (cd "$tmp/work" && check origin) >/dev/null 2>&1 || rc=$?
    [ "$rc" -eq 1 ] || { echo "self-test: end to end, an unclaimed code passed" >&2; return 1; }
    # an unreachable remote warns and lets the push through.
    git -C "$tmp/work" remote set-url origin "$tmp/gone.git"
    rc=0; out="$(cd "$tmp/work" && check origin 2>&1)" || rc=$?
    [ "$rc" -eq 0 ] && [[ "$out" == *"NOT VERIFIED"* ]] \
        || { echo "self-test: unreachable remote gave rc=$rc: $out" >&2; return 1; }
    echo "diag-codes-check: self-test OK (clash, unclaimed, founding, claimed, unreachable)"
}

# check <remote|""> — the real check, in the current directory's repository.
check() {
    local remote="$1" branch main="" claims="-" listing problems rc=0
    [ -f "$LOCK_REL" ] || return 0
    branch="$(cat "$LOCK_REL")"
    if git rev-parse -q --verify "refs/remotes/origin/main^{commit}" >/dev/null 2>&1; then
        if nros_git_history_run 30 rev-parse -q --verify "refs/remotes/origin/main:$LOCK_REL" >/dev/null 2>&1; then
            # One path at one commit: a lazy fetch is a handful of objects,
            # bounded by the deadline.
            main="$(nros_git_history_run 30 show "refs/remotes/origin/main:$LOCK_REL")" || {
                echo "diag-codes-check: NOT VERIFIED (a): could not read main's codes.lock" >&2
                main=""
            }
        fi
    else
        echo "diag-codes-check: NOT VERIFIED (a): no local origin/main to compare with (git fetch origin)" >&2
    fi
    if [ -n "$remote" ]; then
        if listing="$(GIT_TERMINAL_PROMPT=0 git ls-remote "$remote" "$REF_NS/*" 2>/dev/null)"; then
            claims="$(awk '{n=$2; sub(".*/", "", n); print n}' <<<"$listing")"
        else
            echo "diag-codes-check: NOT VERIFIED (b): '$remote' is unreachable, so new codes' claims are unchecked" >&2
        fi
    fi
    problems="$(judge "$branch" "$main" "$claims")" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "diag-codes-check: $LOCK_REL:" >&2
        printf '%s\n' "$problems" | sed 's/^/  /' >&2
        return 1
    fi
    return 0
}

# The self-test runs on every path, so its negative controls cannot decay into
# a comment; it costs a few scratch repositories, well under a second.
case "${1:-origin}" in
    --self-test) self_test ;;
    --no-remote)
        self_test >/dev/null
        check "" && echo "diag-codes-check: OK (self-test and the clash check; no remote asked)"
        ;;
    -*) echo "usage: $0 [<remote> | --no-remote | --self-test]" >&2; exit 2 ;;
    *)
        self_test >/dev/null
        check "${1:-origin}"
        ;;
esac
