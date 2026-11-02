#!/usr/bin/env bash
# issue 2210 — `runner-bootstrap.sh` refreshes the runner's `src` checkout, and
# that checkout is a CACHE: nobody edits it. A build had rewritten a tracked
# lock inside the `play_launch` submodule there, and the next refresh stopped at
# "Your local changes ... would be overwritten by checkout", which left runner
# 118 offline until someone discarded the file by hand.
#
# Drives step 2 alone (`--sync-src <dir>`) against a local origin whose
# superproject AND submodule both move over a dirtied file, and asserts the
# refresh lands on the new pins, leaves nothing modified, and NAMES what it
# discarded (so whatever wrote it can still be found). ~1 s, tmpdir.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bootstrap="$repo_root/scripts/ci/runner-bootstrap.sh"
. "$repo_root/scripts/lib/grep-q.sh"
. "$repo_root/scripts/lib/git-hook-env.sh"
nros_clear_inherited_git_env

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
checks=0
failures=0
ok()   { echo "  [ok]   $1"; checks=$((checks + 1)); }
fail() { echo "  [FAIL] $1" >&2; failures=$((failures + 1)); checks=$((checks + 1)); }

# A local origin only: no global config, file:// submodules allowed.
export GIT_CONFIG_GLOBAL="$work/gitconfig" GIT_CONFIG_NOSYSTEM=1
git config --file "$GIT_CONFIG_GLOBAL" protocol.file.allow always
git config --file "$GIT_CONFIG_GLOBAL" user.name t
git config --file "$GIT_CONFIG_GLOBAL" user.email t@t
git config --file "$GIT_CONFIG_GLOBAL" init.defaultBranch main

sub="$work/play_launch"
git init -q "$sub"
echo 'lock v1' > "$sub/Cargo.lock"
git -C "$sub" add Cargo.lock && git -C "$sub" commit -qm sub1

origin="$work/origin"
git init -q "$origin"
echo 'v1' > "$origin/README"
git -C "$origin" add README
git -C "$origin" submodule add -q "file://$sub" packages/cli/third-party/play_launch
git -C "$origin" commit -qm super1

src="$work/src/nano-ros"
mkdir -p "$(dirname "$src")"
out1="$(NROS_RUNNER_REMOTE="file://$origin" "$bootstrap" --sync-src "$src" 2>&1)"
rc=$?
if [ "$rc" -eq 0 ] && [ -f "$src/packages/cli/third-party/play_launch/Cargo.lock" ]; then
    ok "first refresh clones the checkout and its play_launch submodule"
else
    fail "first refresh failed (rc=$rc):
$out1"
fi

# A build dirties the cache — one file in the superproject, one in the
# submodule, the shape 118 hit — and then origin moves both of them.
echo 'written by a build' > "$src/README"
echo 'rewritten by a build' > "$src/packages/cli/third-party/play_launch/Cargo.lock"
echo 'lock v2' > "$sub/Cargo.lock"
git -C "$sub" commit -qam sub2
git -C "$origin/packages/cli/third-party/play_launch" pull -q
echo 'v2' > "$origin/README"
git -C "$origin" commit -qam super2

out2="$(NROS_RUNNER_REMOTE="file://$origin" "$bootstrap" --sync-src "$src" 2>&1)"
rc=$?
if [ "$rc" -eq 0 ]; then
    ok "a refresh over build-dirtied tracked files succeeds"
else
    fail "a refresh over build-dirtied tracked files failed (rc=$rc):
$out2"
fi
if [ "$(git -C "$src" rev-parse HEAD)" = "$(git -C "$origin" rev-parse HEAD)" ] \
    && [ "$(cat "$src/README")" = v2 ] \
    && [ "$(cat "$src/packages/cli/third-party/play_launch/Cargo.lock")" = 'lock v2' ]; then
    ok "the superproject and the submodule both land on origin's new pins"
else
    fail "the refresh did not land on the new pins (README=$(cat "$src/README"), lock=$(cat "$src/packages/cli/third-party/play_launch/Cargo.lock"))"
fi
if [ -z "$(git -C "$src" status --porcelain --untracked-files=no --ignore-submodules=none)" ]; then
    ok "nothing is left modified"
else
    fail "the checkout is still dirty: $(git -C "$src" status --porcelain --untracked-files=no)"
fi
if nros_grep_q -F -- 'README' <<<"$out2" \
    && nros_grep_q -- 'packages/cli/third-party/play_launch: .*Cargo\.lock' <<<"$out2"; then
    ok "the refresh names each discarded file, the submodule's included"
else
    fail "the refresh did not name what it discarded:
$out2"
fi

# The same pins over a dirty submodule file: --force must still check it out,
# because a plain update skips a submodule already at its pin.
echo 'rewritten again' > "$src/packages/cli/third-party/play_launch/Cargo.lock"
NROS_RUNNER_REMOTE="file://$origin" "$bootstrap" --sync-src "$src" >/dev/null 2>&1
if [ "$(cat "$src/packages/cli/third-party/play_launch/Cargo.lock")" = 'lock v2' ]; then
    ok "a dirty submodule already at its pin is restored too"
else
    fail "a dirty submodule already at its pin was left dirty"
fi

echo
if [ "$failures" -eq 0 ]; then
    echo "runner-bootstrap-src-sync: $checks check(s) passed"
else
    echo "runner-bootstrap-src-sync: $failures of $checks check(s) FAILED" >&2
    exit 1
fi
