#!/usr/bin/env bash
# Issue 2306 — `nros_submodule_provision_state` against every shape a
# submodule's checkout can be left in, on a synthetic superproject built here.
#
# The two rows that motivated it: an interrupted checkout on its pin (HEAD
# right, every tracked file staged as deleted) and one on another line (HEAD at
# upstream `main`). Both used to read as provisioned. `behind` and `ahead` are
# asserted NOT red, because a developer's un-updated or mid-fork submodule must
# not refuse a lane.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/git-hook-env.sh
. "$here/scripts/lib/git-hook-env.sh"
nros_clear_inherited_git_env
# shellcheck source=scripts/lib/submodule-provision.sh
. "$here/scripts/lib/submodule-provision.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/submodule-provision-XXXXXX")"
trap 'rm -rf "$work"' EXIT
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
g() { git -c init.defaultBranch=main -c protocol.file.allow=always "$@"; }

# Upstream with a pin (c2), one commit after it (c3), and an unrelated line.
g init -q "$work/up"
for i in 1 2; do echo "$i" >"$work/up/f$i"; g -C "$work/up" add "f$i"; g -C "$work/up" commit -qm "c$i"; done
pin="$(g -C "$work/up" rev-parse HEAD)"
echo 3 >"$work/up/f3"; g -C "$work/up" add f3; g -C "$work/up" commit -qm c3
g -C "$work/up" checkout -q --orphan other
g -C "$work/up" rm -rq --cached .; g -C "$work/up" clean -fdq
echo x >"$work/up/x"; g -C "$work/up" add x; g -C "$work/up" commit -qm other
g -C "$work/up" checkout -q main

g init -q "$work/super"
g -C "$work/super" submodule -q add "$work/up" sub
g -C "$work/super/sub" checkout -q "$pin"
g -C "$work/super" add sub; g -C "$work/super" commit -qm pin
cd "$work/super"

fails=0
expect() {
    local what="$1" want="$2" got red=no
    got="$(nros_submodule_provision_state sub)"
    nros_submodule_state_is_red "$got" && red=yes
    if [ "${got%%:*}" != "${want%% *}" ] || [ "$red" != "${want##* }" ]; then
        echo "[FAIL] $what: got $got (red=$red), want ${want%% *} (red=${want##* })"
        fails=$((fails + 1))
    else
        echo "  ok    $what: $got"
    fi
}

expect "on the pin, complete" "ok no"
[ "$(nros_submodule_provision_state nowhere)" = not-submodule ] \
    || { echo "[FAIL] a path that names no submodule"; fails=$((fails + 1)); }

rm "sub/f1"
expect "a tracked file deleted from the tree" "incomplete yes"
g -C sub checkout -q -- f1

g -C sub rm -q --cached f1 f2
expect "on the pin, every file staged as deleted (the micro-cdr row)" "incomplete yes"
g -C sub reset -q

g -C sub checkout -q main
expect "ahead of the pin (a fork fix in progress)" "ahead no"
g -C sub checkout -q "$pin"

g -C sub checkout -q other
expect "on an unrelated line (upstream main)" "diverged yes"
g -C sub checkout -q "$pin"

# Behind: pin the superproject to c3, leave the checkout at c2.
g -C sub checkout -q main; g add sub; g commit -qm bump; g -C sub checkout -q "$pin"
expect "behind the pin (left un-updated)" "behind no"
g -C sub checkout -q main

echo modified >"sub/f2"
expect "a modified file is someone's work, not a broken checkout" "ok no"
g -C sub checkout -q -- f2

g submodule -q deinit -f sub
expect "not initialised" "missing yes"

if [ "$fails" -ne 0 ]; then
    echo "submodule-provision: $fails case(s) failed"
    exit 1
fi
echo "submodule-provision: OK"
