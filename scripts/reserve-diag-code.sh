#!/usr/bin/env bash
#
# Reserve the next free diagnostic code (`NROS-NNNN`), ATOMICALLY, across every
# parallel session. `just code-new <Name>` runs this.
#
# WHY
#
# A diagnostic code is chosen once and never changes (RFC-0129 D3): it is
# printed in refusals, quoted in issues and searched for by users. Reading "the
# highest code in codes.lock, plus one" is the check-then-act race that made
# issue ids collide seven times: two sessions read the same lock and both
# write the next number. So a number is owned by a ref on origin, the same
# compare-and-swap `scripts/reserve-issue-id.sh` uses:
#
#   git push <unique commit>:refs/diag-codes/NNNN
#
# Git refuses to create a ref that already exists, and the commit object is
# unique per attempt (timestamp, pid, host, name), so push success IS
# ownership. The refs live outside `refs/tags/`, so nobody fetches them by
# default.
#
# The highest number in use is the larger of the committed lock
# (`packages/tooling/nros-diagnostic-codes/codes.lock`, which lists retired
# codes too) and the highest reservation on the remote.
#
# NO FALLBACK
#
# A code is CLAIMED on origin or it is not allocated (user rule: codes are never
# reused). If the push fails for any reason other than "already taken" (no
# network, no push permission, a read-only mirror), this prints what failed and
# how to retry, prints NO code, and exits 1. The issue-id script falls back to
# "local maximum + 1"; a diagnostic code may not, because a number printed to
# users cannot be taken back once a second session has shipped it too.
#
# Contention ("already taken") is the CAS working: the next number is tried, up
# to MAX_ATTEMPTS.
#
# The pre-push hook (`scripts/ci/diag-codes-check.sh`) refuses a branch whose
# codes.lock adds a number with no claim ref, so a code written by hand without
# this script does not land either. NROS-0001..NROS-0006 were allocated in the
# founding commit, before claims existed; codes.lock's `# founding:` line
# records that range and the guard exempts it.
#
# NROS_DIAG_REMOTE names the remote (default `origin`); `--self-test` points it
# at scratch bare repositories (success, contention, unreachable).

set -euo pipefail
cd "$(dirname "$0")/.."
# The self-test builds throwaway repositories: never let an inherited GIT_DIR
# point those commands at this checkout (issue 0986).
# shellcheck source=scripts/lib/git-hook-env.sh
. scripts/lib/git-hook-env.sh
nros_clear_inherited_git_env

self_test() {
    local tmp self out rc
    self="$PWD/scripts/reserve-diag-code.sh"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    printf '# founding: NROS-0001..NROS-0006\nNROS-0001 A live\nNROS-0006 F live\n' >"$tmp/codes.lock"
    git init -q --bare "$tmp/remote.git"
    git init -q --bare "$tmp/objects.git"
    export NROS_DIAG_LOCK="$tmp/codes.lock" NROS_DIAG_OBJECT_REPO="$tmp/objects.git"

    # 1. success: the first claim after the lock's highest code.
    out="$(NROS_DIAG_REMOTE="$tmp/remote.git" "$self" FooBar 2>/dev/null)"
    [ "$out" = "NROS-0007" ] || { echo "self-test: success printed '$out', want NROS-0007" >&2; return 1; }
    git --git-dir="$tmp/remote.git" rev-parse -q --verify refs/diag-codes/0007 >/dev/null \
        || { echo "self-test: no refs/diag-codes/0007 on the remote" >&2; return 1; }

    # 2. contention: 0008 is taken by a session this one cannot see yet (the
    #    listing is skipped), so the push is rejected and 0009 is claimed.
    git --git-dir="$tmp/remote.git" update-ref refs/diag-codes/0008 refs/diag-codes/0007
    printf 'NROS-0007 G live\n' >>"$tmp/codes.lock"
    out="$(NROS_DIAG_REMOTE="$tmp/remote.git" NROS_DIAG_SKIP_LISTING=1 "$self" Bar 2>/dev/null)"
    [ "$out" = "NROS-0009" ] || { echo "self-test: contention printed '$out', want NROS-0009" >&2; return 1; }

    # 3. unreachable: non-zero, and no code on stdout.
    rc=0
    out="$(NROS_DIAG_REMOTE="$tmp/no-such-remote.git" "$self" Baz 2>/dev/null)" || rc=$?
    [ "$rc" -ne 0 ] && [ -z "$out" ] \
        || { echo "self-test: unreachable remote gave rc=$rc, stdout '$out'" >&2; return 1; }
    echo "reserve-diag-code: self-test OK (success, contention, unreachable)"
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit $?
fi

REF_NS="refs/diag-codes"
REMOTE="${NROS_DIAG_REMOTE:-origin}"
LOCK="${NROS_DIAG_LOCK:-packages/tooling/nros-diagnostic-codes/codes.lock}"
# The repository the claim object is written in and pushed from: this checkout,
# or (the self-test) a scratch one, so a self-test never writes this checkout's
# object store.
OBJ_REPO=()
[ -z "${NROS_DIAG_OBJECT_REPO:-}" ] || OBJ_REPO=(-C "$NROS_DIAG_OBJECT_REPO")
MAX_ATTEMPTS=25

name="${1:-}"
if [ -n "$name" ] && ! [[ "$name" =~ ^[A-Z][A-Za-z0-9]*$ ]]; then
    echo "usage: $0 [Name]   (Name is the UpperCamelCase variant, e.g. PlatformNoFamily)" >&2
    exit 2
fi

# Highest code in the committed lock, live or retired: a retired code still
# owns its number. Read line by line in bash: no producer process to lose.
highest_lock_code() {
    local max=0 n line
    [ -f "$LOCK" ] || { printf '0'; return; }
    while IFS= read -r line; do
        [[ "$line" =~ ^NROS-([0-9]{4})[[:space:]] ]] || continue
        n=$((10#${BASH_REMATCH[1]}))
        [ "$n" -gt "$max" ] && max="$n"
    done <"$LOCK"
    printf '%s' "$max"
}

# Highest number reserved by any session, committed or not. An unreachable
# remote reads as no reservations: the push below then fails and says so.
# NROS_DIAG_SKIP_LISTING=1 skips the listing (the self-test's contention case).
highest_reserved_code() {
    local max=0 n ref refs
    if [ -n "${NROS_DIAG_SKIP_LISTING:-}" ]; then
        printf '0'
        return
    fi
    refs="$(git ls-remote "$REMOTE" "$REF_NS/*" 2>/dev/null || true)"
    while IFS= read -r ref; do
        ref="${ref##*/}"
        [[ "$ref" =~ ^[0-9]{4}$ ]] || continue
        n=$((10#$ref))
        [ "$n" -gt "$max" ] && max="$n"
    done <<<"$refs"
    printf '%s' "$max"
}

# A commit object no other session can reproduce.
unique_object() {
    local n="$1" empty_tree
    empty_tree="$(git "${OBJ_REPO[@]}" hash-object -t tree /dev/null)"
    printf 'reserve diagnostic code NROS-%04d\n\nhost: %s\npid: %s\nwhen: %s\nname: %s\n' \
        "$n" "$(hostname 2>/dev/null || echo unknown)" "$$" \
        "$(date +%s%N 2>/dev/null || date +%s)" "${name:-<unspecified>}" |
        git "${OBJ_REPO[@]}" commit-tree "$empty_tree"
}

hint() {
    local code="$1"
    echo "  declare it in packages/tooling/nros-diagnostic-codes/src/codes.rs as" >&2
    echo "      $code ${name:-<Name>} { title: \"…\", surfaces: [Cli], remedy: \"…\" }" >&2
    echo "  write its explanation in packages/tooling/nros-diagnostic-codes/explain/NROS-$code.md," >&2
    echo "  then regenerate the views: cargo run -p nros-diagnostic-codes --features std --bin nros-diagnostics-gen" >&2
}

lock_max="$(highest_lock_code)"
reserved_max="$(highest_reserved_code)"
candidate=$(( (lock_max > reserved_max ? lock_max : reserved_max) + 1 ))

attempt=0
while [ "$attempt" -lt "$MAX_ATTEMPTS" ]; do
    code="$(printf '%04d' "$candidate")"
    if [ "$candidate" -gt 9999 ]; then
        echo "[FAIL] the code space NROS-0001..NROS-9999 is exhausted." >&2
        exit 1
    fi
    obj="$(unique_object "$candidate")"

    if err="$(git "${OBJ_REPO[@]}" push "$REMOTE" "$obj:$REF_NS/$code" 2>&1)"; then
        printf 'NROS-%s\n' "$code"
        echo "reserved NROS-$code on $REMOTE ($REF_NS/$code)" >&2
        hint "$code"
        exit 0
    fi

    # Taken by another session between our read and our write: step on.
    shopt -s nocasematch
    if [[ "$err" =~ already\ exists|non-fast-forward|fetch\ first|rejected ]]; then
        shopt -u nocasematch
        candidate=$((candidate + 1))
        attempt=$((attempt + 1))
        continue
    fi
    shopt -u nocasematch

    echo "[FAIL] could not claim a code on '$REMOTE'; no code is allocated:" >&2
    printf '%s\n' "$err" | sed 's/^/       /' >&2
    echo "" >&2
    echo "       A code is claimed on origin or not allocated at all: never pick" >&2
    echo "       the next number by hand. Fix what failed above and run" >&2
    echo "       \`just code-new ${name:-<Name>}\` again:" >&2
    echo "         - no network: reconnect, then retry;" >&2
    echo "         - no push permission (a fork or read-only mirror): run it where" >&2
    echo "           '$REMOTE' accepts pushes, or ask someone who can to claim it." >&2
    exit 1
done

echo "[FAIL] $MAX_ATTEMPTS consecutive codes were already reserved." >&2
echo "       Check \`git ls-remote $REMOTE '$REF_NS/*'\` for stale reservations." >&2
exit 1
