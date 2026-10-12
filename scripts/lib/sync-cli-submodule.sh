#!/usr/bin/env bash
# Bring the in-tree CLI's submodule (play_launch) to the commit this tree
# records — issues 1081 and 1841. `just setup`'s `_setup-common` calls it.
#
# THREE STATES, AND WHO OWNS THE WORKTREE DECIDES THE THIRD
#
#   `-` not initialised            → init it (non-recursive, RFC-0060).
#   `+` at another commit, clean   → update it; there is nothing to lose.
#   `+` at another commit, DIRTY   → depends on whose worktree it is.
#
# In a DEVELOPER tree the dirt may be work, and discarding work in a submodule
# is a human decision (AGENTS.md), so this refuses and says how to resolve it
# (issue 1081). Resolving against the old commit instead is worse: the lockfiles
# describe the pin, so it surfaces several steps later as
# `cannot update the lock file … --locked`.
#
# In a CI JOB's workspace nobody authors anything. A self-hosted runner's
# workspace PERSISTS between jobs, `actions/checkout` runs without submodules,
# and a build leaves the submodule worktree modified — so after the pin moved,
# every L3 job on every self-hosted runner refused here, and the advice it
# printed ("commit or stash") had no one to address (issue 1841, measured on
# newslab-118, -133 and server-241 after #2038). There the dirt is residue of an
# earlier job, so it is discarded — and LISTED first, so the log says what it
# was rather than leaving the next reader to guess.
#
# "CI job" is `GITHUB_ACTIONS=true`, which GitHub sets on every job and no
# developer shell has. Asking a per-workflow opt-in instead would need every
# self-hosted job to remember it, which is how 1841 started: 1081 fixed one
# shape of the persistent workspace and the next shape had no step.
#
# Usage: sync-cli-submodule.sh            (from the checkout root)
#        sync-cli-submodule.sh --selftest
set -euo pipefail

SUB="packages/cli/third-party/play_launch"

nros_sync_cli_submodule() {
    local sub="${1:-$SUB}" state
    state="$(git submodule status --cached "$sub" 2>/dev/null | cut -c1)"
    case "$state" in
        -)
            echo "setup: initialising in-tree CLI submodule ($sub)"
            git submodule update --init "$sub"
            ;;
        +)
            local dirt here
            dirt="$(git -C "$sub" status --porcelain 2>/dev/null)"
            here="$(git -C "$sub" rev-parse --short HEAD)"
            if [ -z "$dirt" ]; then
                echo "setup: $sub is at $here, not the recorded pin — updating (worktree clean)"
                git submodule update "$sub"
            elif [ "${GITHUB_ACTIONS:-}" = "true" ]; then
                echo "setup: $sub is at $here, not the recorded pin, and dirty — a CI workspace"
                echo "  authors nothing, so this is an earlier job's residue (issue 1841). Discarding:"
                printf '%s\n' "$dirt" | sed 's/^/    /'
                git -C "$sub" reset -q --hard
                # No -x: ignored build output (cargo target dirs) is a cache,
                # not dirt, and `status --porcelain` never listed it.
                git -C "$sub" clean -q -ffd
                git submodule update --force "$sub"
            else
                echo "setup: FAILED — $sub is at $here, not the pin this tree records," >&2
                echo "  and it has uncommitted changes, so this cannot move it for you (issue 1081)." >&2
                echo "  Building against it resolves a graph the lockfiles do not describe, which" >&2
                echo "  surfaces as \`cannot update the lock file … --locked\` several steps later." >&2
                echo "" >&2
                echo "  Commit or stash the submodule work, then:  git submodule update $sub" >&2
                return 1
            fi
            ;;
    esac
}

# Builds throwaway repositories, so the inherited git environment goes first
# (issue 0986): a GIT_DIR from a hook would point every command below at the
# caller's repository.
selftest() {
    local root tmp fails=0
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
    # shellcheck source=scripts/lib/git-hook-env.sh
    . "$root/scripts/lib/git-hook-env.sh"
    nros_clear_inherited_git_env
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    local g=(git -c user.name=t -c user.email=t@t -c protocol.file.allow=always
        -c init.defaultBranch=main)

    "${g[@]}" init -q "$tmp/dep"
    echo one >"$tmp/dep/f"
    "${g[@]}" -C "$tmp/dep" add f
    "${g[@]}" -C "$tmp/dep" commit -qm one
    local old
    old="$(git -C "$tmp/dep" rev-parse HEAD)"
    echo two >"$tmp/dep/f"
    "${g[@]}" -C "$tmp/dep" commit -qam two
    local pin
    pin="$(git -C "$tmp/dep" rev-parse HEAD)"

    "${g[@]}" init -q "$tmp/super"
    "${g[@]}" -C "$tmp/super" submodule add -q "$tmp/dep" sub
    "${g[@]}" -C "$tmp/super" commit -qm pin

    # The persistent-workspace state: the checkout sits on the OLD commit with
    # a modified tracked file and an untracked one.
    reset_case() {
        git -C "$tmp/super/sub" checkout -q -f "$old"
        echo residue >"$tmp/super/sub/f"
        echo stray >"$tmp/super/sub/untracked"
    }
    expect() { # <name> <want-rc> <want-head> <rc> <head>
        if [ "$4" != "$2" ] || [ "$5" != "$3" ]; then
            echo "  FAIL $1: rc=$4 head=${5:0:8} (want rc=$2 head=${3:0:8})"
            fails=$((fails + 1))
        else
            echo "  ok   $1"
        fi
    }
    local rc head

    reset_case
    rc=0
    (cd "$tmp/super" && GITHUB_ACTIONS='' nros_sync_cli_submodule sub) >/dev/null 2>&1 || rc=$?
    head="$(git -C "$tmp/super/sub" rev-parse HEAD)"
    expect "developer tree, dirty, off pin -> refuses, keeps the work" 1 "$old" "$rc" "$head"
    [ "$(cat "$tmp/super/sub/f")" = residue ] || {
        echo "  FAIL developer tree: the modified file was touched"
        fails=$((fails + 1))
    }

    reset_case
    rc=0
    (cd "$tmp/super" && GITHUB_ACTIONS=true nros_sync_cli_submodule sub) >/dev/null 2>&1 || rc=$?
    head="$(git -C "$tmp/super/sub" rev-parse HEAD)"
    expect "CI workspace, dirty, off pin -> reaches the pin" 0 "$pin" "$rc" "$head"
    [ -z "$(git -C "$tmp/super/sub" status --porcelain)" ] || {
        echo "  FAIL CI workspace: still dirty after the sync"
        fails=$((fails + 1))
    }

    git -C "$tmp/super/sub" checkout -q -f "$old"
    rm -f "$tmp/super/sub/untracked"
    rc=0
    (cd "$tmp/super" && GITHUB_ACTIONS='' nros_sync_cli_submodule sub) >/dev/null 2>&1 || rc=$?
    head="$(git -C "$tmp/super/sub" rev-parse HEAD)"
    expect "developer tree, clean, off pin -> updates" 0 "$pin" "$rc" "$head"

    if [ "$fails" -ne 0 ]; then
        echo "sync-cli-submodule selftest: $fails FAILED"
        return 1
    fi
    echo "sync-cli-submodule selftest: ok"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    if [ "${1:-}" = "--selftest" ]; then
        selftest
    else
        # The selftest runs on the normal path too (check-gate-selftests): a
        # negative control only `--selftest` reaches decays into a comment. It
        # costs ~0.1 s of throwaway repositories and is silent unless it fails.
        if ! out="$(selftest 2>&1)"; then
            printf '%s\n' "$out" >&2
            exit 1
        fi
        nros_sync_cli_submodule
    fi
fi
