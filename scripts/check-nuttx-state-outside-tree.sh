#!/usr/bin/env bash
# nano-ros's own NuttX build state lives BESIDE the NuttX tree, never in it.
#
# `build-nuttx.sh`'s lock, its `HEAD:sha256(defconfig)` markers and the
# per-config export snapshots used to be written at the root of the vendored
# `third-party/nuttx/nuttx` submodule, which then read as dirty in every
# checkout that built NuttX. `worktree-prune` grew an allow-list for them
# (issue 2611). They now live in `<tree>.nros-build/` (issue 2622), and every
# reader reaches them through ONE locator per language:
#
#   shell  scripts/lib/nuttx-state.sh             nros_nuttx_export_snapshot
#   cmake  packages/api/nros-c/cmake/nros-nuttx.cmake  nros_nuttx_export_snapshot
#   Rust   packages/tooling/nros-build-paths/src/lib.rs  nuttx_export_snapshot
#
# Checks (no build):
#   1. SPELLINGS: outside the three locators, no code line spells a snapshot
#      name (`nros-nuttx-export-`) or an in-tree marker / lock name
#      (`.nros-nuttx-build`) — a reader that joins one to a tree path by hand
#      is the five-spellings state this replaced. Comment lines may name them,
#      and a line that only DESCRIBES one carries `nuttx-state-literal-ok:`
#      with its reason.
#   2. AGREEMENT: the shell and cmake locators give the same path for the same
#      tree and id (the Rust one is pinned by its own unit test).
#
# The negative control runs first, on the normal path: check 1 against a file
# that spells the snapshot name by hand must fail.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root" || exit 2
# shellcheck source=scripts/lib/grep-q.sh
. "$repo_root/scripts/lib/grep-q.sh"
# shellcheck source=scripts/lib/nuttx-state.sh
. "$repo_root/scripts/lib/nuttx-state.sh"

LOCATORS=(
    scripts/lib/nuttx-state.sh
    packages/api/nros-c/cmake/nros-nuttx.cmake
    packages/tooling/nros-build-paths/src/lib.rs
    # This gate names what it forbids.
    scripts/check-nuttx-state-outside-tree.sh
)
PATTERN='nros-nuttx-export-|[.]nros-nuttx-build'

# Code lines (not comments) among `git grep -n` output lines on stdin.
code_lines() {
    awk -F: '{
        line = $0; sub(/^[^:]*:[0-9]+:/, "", line)
        t = line; sub(/^[ \t]+/, "", t)
        if (t ~ /^(#|\/\/|\/\*|\*)/) next
        if (line ~ /nuttx-state-literal-ok:/) next
        print
    }'
}

# check 1 over the git grep output in $1.
offenders() {
    local hits="$1" f
    printf '%s\n' "$hits" | awk 'NF' | code_lines | while IFS= read -r l; do
        f="${l%%:*}"
        case " ${LOCATORS[*]} " in
            *" $f "*) ;;
            *) printf '%s\n' "$l" ;;
        esac
    done
}

self_test() {
    local fake bad
    fake="packages/x/src/fake.rs:12:    let p = tree.join(format!(\"nros-nuttx-export-{id}\"));"
    bad="$(offenders "$fake")"
    if [ -z "$bad" ]; then
        echo "self-test FAILED: a hand-spelled snapshot path passed check 1." >&2
        return 1
    fi
    bad="$(offenders "packages/x/src/fake.rs:3:// the nros-nuttx-export-<id> snapshot")"
    if [ -n "$bad" ]; then
        echo "self-test FAILED: a comment naming the snapshot failed check 1." >&2
        return 1
    fi
    echo "self-test OK: check 1 refuses a hand-spelled path and passes a comment."
}

main() {
    self_test || exit 1
    local fail=0 hits bad
    hits=""
    nros_git_grep hits -nE "$PATTERN" -- \
        '*.rs' '*.sh' '*.cmake' 'CMakeLists.txt' '*.py' '*.just' 'justfile' 'Makefile' '*.mk' \
        ':!docs' || exit 2
    bad="$(offenders "$hits")"
    if [ -n "$bad" ]; then
        echo "check-nuttx-state-outside-tree: a NuttX state path spelled outside its locator" >&2
        echo "  (use nros_nuttx_export_snapshot / nros_build_paths::nuttx_export_snapshot):" >&2
        printf '%s\n' "$bad" | sed 's/^/    /' >&2
        fail=1
    fi

    if command -v cmake >/dev/null 2>&1; then
        local tmp sh cm
        tmp="$(mktemp -d "${TMPDIR:-/tmp}/nros-nuttx-state.XXXXXX")" || exit 2
        # shellcheck disable=SC2064
        trap "rm -rf '$tmp'" EXIT
        sh="$(nros_nuttx_export_snapshot /x/third-party/nuttx/nuttx arm-smp)"
        printf '%s\n' \
            "include(\"$repo_root/packages/api/nros-c/cmake/nros-nuttx.cmake\" OPTIONAL RESULT_VARIABLE _r)" \
            'if(NOT COMMAND nros_nuttx_export_snapshot)' \
            '  message(FATAL_ERROR "nros_nuttx_export_snapshot not defined")' \
            'endif()' \
            'nros_nuttx_export_snapshot(_s "/x/third-party/nuttx/nuttx" "arm-smp")' \
            'message("${_s}")' > "$tmp/probe.cmake"
        cm="$(cmake -P "$tmp/probe.cmake" 2>&1 | tail -1)"
        if [ "$sh" != "$cm" ]; then
            echo "check-nuttx-state-outside-tree: the shell and cmake locators disagree:" >&2
            echo "    shell: $sh" >&2
            echo "    cmake: $cm" >&2
            fail=1
        fi
    else
        echo "check-nuttx-state-outside-tree: cmake absent; locator agreement not checked" >&2
        fail=1
    fi

    if [ "$fail" -ne 0 ]; then
        echo "check-nuttx-state-outside-tree: FAILED (issue 2622)" >&2
        exit 1
    fi
    echo "check-nuttx-state-outside-tree: OK — every NuttX state path goes through its locator; shell and cmake agree"
}

main "$@"
