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
# DEGRADATION
#
# If the push fails for any reason other than "already taken" (no network, no
# push permission, a read-only mirror), this says so and falls back to the lock
# maximum + 1. That fallback is the racy behaviour, and it is announced; the
# `check-diagnostics` gate still refuses a lock that renumbers a code main has.
#
# NROS_DIAG_REMOTE names the remote (default `origin`); the self-test points it
# at a scratch bare repository.

set -euo pipefail
cd "$(dirname "$0")/.."

REF_NS="refs/diag-codes"
REMOTE="${NROS_DIAG_REMOTE:-origin}"
LOCK="${NROS_DIAG_LOCK:-packages/tooling/nros-diagnostic-codes/codes.lock}"
MAX_ATTEMPTS=25

name="${1:-}"
if [ -n "$name" ] && ! printf '%s' "$name" | grep -qE '^[A-Z][A-Za-z0-9]*$'; then
    echo "usage: $0 [Name]   (Name is the UpperCamelCase variant, e.g. PlatformNoFamily)" >&2
    exit 2
fi

# Highest code in the committed lock, live or retired: a retired code still
# owns its number.
highest_lock_code() {
    local max=0 n
    [ -f "$LOCK" ] || { printf '0'; return; }
    while IFS= read -r n; do
        n=$((10#$n))
        [ "$n" -gt "$max" ] && max="$n"
    done < <(grep -oE '^NROS-[0-9]{4}' "$LOCK" | cut -c6-)
    printf '%s' "$max"
}

# Highest number reserved by any session, committed or not.
highest_reserved_code() {
    local max=0 n
    while IFS= read -r ref; do
        n="$(printf '%s' "${ref##*/}" | grep -oE '^[0-9]{4}$' || true)"
        [ -z "$n" ] && continue
        n=$((10#$n))
        [ "$n" -gt "$max" ] && max="$n"
    done < <(git ls-remote "$REMOTE" "$REF_NS/*" 2>/dev/null | awk '{print $2}')
    printf '%s' "$max"
}

# A commit object no other session can reproduce.
unique_object() {
    local n="$1" empty_tree
    empty_tree="$(git hash-object -t tree /dev/null)"
    printf 'reserve diagnostic code NROS-%04d\n\nhost: %s\npid: %s\nwhen: %s\nname: %s\n' \
        "$n" "$(hostname 2>/dev/null || echo unknown)" "$$" \
        "$(date +%s%N 2>/dev/null || date +%s)" "${name:-<unspecified>}" |
        git commit-tree "$empty_tree"
}

hint() {
    local code="$1"
    echo "  declare it in packages/tooling/nros-diagnostic-codes/src/codes.rs as" >&2
    echo "      $code ${name:-<Name>} { title: \"…\", surfaces: [Cli], remedy: \"…\" }" >&2
    echo "  write its explanation in packages/tooling/nros-diagnostic-codes/explain/NROS-$code.md," >&2
    echo "  then regenerate the views: cargo run -p nros-diagnostic-codes --features gen --bin nros-diagnostics-gen" >&2
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

    if err="$(git push "$REMOTE" "$obj:$REF_NS/$code" 2>&1)"; then
        printf 'NROS-%s\n' "$code"
        echo "reserved NROS-$code on $REMOTE ($REF_NS/$code)" >&2
        hint "$code"
        exit 0
    fi

    # Taken by another session between our read and our write: step on.
    if grep -qiE "already exists|non-fast-forward|fetch first|rejected" <<<"$err"; then
        candidate=$((candidate + 1))
        attempt=$((attempt + 1))
        continue
    fi

    echo "[WARN] could not reserve a code on '$REMOTE':" >&2
    printf '%s\n' "$err" | sed 's/^/       /' >&2
    echo "" >&2
    echo "       Falling back to the lock maximum + 1. This is the RACY behaviour:" >&2
    echo "       another session may pick the same number. check-diagnostics still" >&2
    echo "       refuses a lock that renumbers a code main already has." >&2
    code="$(printf '%04d' "$((lock_max + 1))")"
    printf 'NROS-%s\n' "$code"
    hint "$code"
    exit 0
done

echo "[FAIL] $MAX_ATTEMPTS consecutive codes were already reserved." >&2
echo "       Check \`git ls-remote $REMOTE '$REF_NS/*'\` for stale reservations." >&2
exit 1
