#!/usr/bin/env bash
# issue 2583 -- the pre-push hook reads each ref line's <local-sha> as a commit
# only when the push publishes one. A deletion (`git push --delete <ref>`)
# carries an all-zero local sha: it must pass without arming any check, while
# a branch update that moves a pin backward is still refused.
#
# Runs the REAL hook in a scratch repository whose four checks
# (issue ids, diag codes, submodule pins, submodule reachability) are stubs
# that log their arguments, so each case asserts both the verdict and which
# checks ran with which shas. `NROS_SKIP_PREPUSH_CHECKS=1` keeps the fast tier
# out. Buildless, tmpdir, <1 s.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failures=0
checks=0
ok()   { echo "  [ok]   $1"; checks=$((checks + 1)); }
fail() { echo "  [FAIL] $1" >&2; failures=$((failures + 1)); checks=$((checks + 1)); }

# shellcheck source=scripts/lib/git-hook-env.sh
. "$repo_root/scripts/lib/git-hook-env.sh"
# shellcheck source=scripts/lib/grep-q.sh
. "$repo_root/scripts/lib/grep-q.sh"
nros_clear_inherited_git_env

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
r="$work/repo"
mkdir -p "$r/.githooks" "$r/scripts/ci" "$r/scripts/lib"
git init -q "$r"
cp "$repo_root/.githooks/pre-push" "$r/.githooks/pre-push"
cp "$repo_root/scripts/lib/git-hook-env.sh" "$r/scripts/lib/git-hook-env.sh"
log="$work/calls.log"
# A stub fails, as the real script does ("could not read the gitlinks at
# '0000…'"), when handed an all-zero sha: that is the refusal issue 2583 hit.
for name in issue-ids-check diag-codes-check submodule-pins-check submodule-commits-reachable; do
    printf '#!/usr/bin/env bash\necho "%s $*" >> "%s"\nfor a in "$@"; do case "$a" in *[!0]*|"") ;; *) exit 1 ;; esac; done\nexit "${STUB_%s_RC:-0}"\n' \
        "$name" "$log" "$(echo "$name" | tr 'a-z-' 'A-Z_')" > "$r/scripts/ci/$name.sh"
    chmod +x "$r/scripts/ci/$name.sh"
done

Z=0000000000000000000000000000000000000000
OLD=1111111111111111111111111111111111111111
NEW=2222222222222222222222222222222222222222

# hook <stdin> [VAR=value ...] -- run the hook; sets $rc and $out, resets the log.
hook() {
    local input="$1"; shift
    : > "$log"
    out="$(cd "$r" && printf '%s' "$input" \
        | env NROS_SKIP_PREPUSH_CHECKS=1 "$@" bash .githooks/pre-push origin 2>&1)"
    rc=$?
}
calls() { cat "$log"; }

# 1. A branch deletion alone: passes, and no check runs.
hook "(delete) $Z refs/heads/release/nros9 $OLD
"
[ "$rc" -eq 0 ] && ok "a branch deletion passes" || fail "a branch deletion was refused (rc $rc): $out"
[ -z "$(calls)" ] && ok "a branch deletion arms no check" || fail "a deletion ran: $(calls)"

# 2. NEGATIVE CONTROL: an update whose pin check refuses (a real rewind) is refused.
hook "refs/heads/x $NEW refs/heads/x $OLD
" STUB_SUBMODULE_PINS_CHECK_RC=1
[ "$rc" -ne 0 ] && ok "an update with a pin rewind is still refused" \
    || fail "a pin rewind was let through"
nros_grep_q -x "submodule-pins-check $OLD $NEW" "$log" \
    && ok "the pin check is asked about the remote and local shas" \
    || fail "pin check not called as expected: $(calls)"
case "$out" in *"a submodule pin would move backward"*) ok "the refusal says why" ;;
    *) fail "the refusal text is missing: $out" ;; esac

# 3. An ordinary update passes and runs the pin and reachability checks.
hook "refs/heads/x $NEW refs/heads/x $OLD
"
[ "$rc" -eq 0 ] && ok "an ordinary update passes" || fail "an ordinary update was refused: $out"
nros_grep_q "^submodule-commits-reachable --changed $OLD" "$log" \
    && ok "an update asks reachability against the remote sha" \
    || fail "reachability not asked: $(calls)"

# 4. A deletion beside an update: only the update's shas reach the checks.
hook "(delete) $Z refs/heads/gone $OLD
refs/heads/x $NEW refs/heads/x $OLD
"
[ "$rc" -eq 0 ] && ok "a deletion beside an update passes" || fail "mixed push refused: $out"
[ "$(grep -c '^submodule-pins-check ' "$log")" -eq 1 ] && nros_grep_q -x "submodule-pins-check $OLD $NEW" "$log" \
    && ok "only the update reaches the pin check" || fail "pin check calls: $(calls)"
nros_grep_q "$Z" "$log" && fail "a zero sha reached a check: $(calls)" \
    || ok "no check sees the deletion's zero sha"

# 5. A tag push arms no branch check, as before.
hook "refs/tags/v1 $NEW refs/tags/v1 $Z
"
[ "$rc" -eq 0 ] && [ -z "$(calls)" ] && ok "a tag push arms no check" \
    || fail "tag push: rc $rc, calls: $(calls)"

echo "pre-push-refs: $checks checks, $failures failed"
[ "$failures" -eq 0 ]
