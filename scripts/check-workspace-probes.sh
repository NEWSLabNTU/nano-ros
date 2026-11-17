#!/usr/bin/env bash
# check-workspace-probes.sh — every tracked workspace's metadata probes pass
# under `NROS_PROBE_FAILURE=refuse` (issue 2608's reach gap).
#
# Since phase-559 W11 a C/C++ metadata probe that fails is a failure, and every
# unattended lane runs with `refuse`, so a component the HOST probe cannot
# compile stops the fixture build that syncs its workspace. W11's measurement
# before merge configured the 18 `examples/workspaces/*` and nothing else, and
# a component under `packages/testing/nros-tests/bins/` (a Zephyr witness that
# included <zephyr/devicetree.h>) took every Zephyr fixture build on main down.
# This runs the same configure over EVERY tracked workspace root that holds a
# C or C++ package, so the next such component is red before it merges.
#
# A root is a directory with a tracked `.colcon_workspace` marker; one with no
# tracked CMakeLists.txt has no C/C++ probe and is not visited. Each root runs
# `nros configure` under `refuse`, and its output goes to a log under the scratch
# dir; a refused root prints the tail of its log.
#
# Usage:
#   check-workspace-probes.sh             every root
#   check-workspace-probes.sh <root>...   only these (for a repro)
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# shellcheck source=scripts/build/cargo.sh
source scripts/build/cargo.sh
# This checkout's CLI, never one from PATH (issue 1850).
if ! cli="$(nros_cli_bin --checkout-only)"; then
    # shellcheck source=scripts/build/check-skip.sh
    source scripts/build/check-skip.sh
    nros_check_skip workspace-probes "no in-tree CLI -- run \`just setup-cli\`"
    exit 0
fi

# The roots to visit in the repository at "$1": every tracked
# `.colcon_workspace` marker's directory that holds a tracked CMakeLists.txt,
# vendored trees excluded. One per line.
list_roots() {
    local repo="$1" marker root cmake
    local -a markers
    mapfile -t markers < <(git -C "$repo" ls-files -- '*.colcon_workspace')
    wait $! || { echo "check-workspace-probes: listing the workspace markers failed" >&2; return 1; }
    for marker in "${markers[@]}"; do
        root="${marker%/.colcon_workspace}"
        case "$root" in third-party/* | packages/cli/third-party/*) continue ;; esac
        # A pathspec `*` matches across `/`, so this finds a nested one too.
        cmake="$(git -C "$repo" ls-files -- "$root/*CMakeLists.txt")" || return 1
        [ -n "$cmake" ] && printf '%s\n' "$root"
    done
    return 0
}

# Negative control, on every run: a repository with a C root, a Rust-only
# root and a vendored C root must yield exactly the C root.
selftest() {
    local t got
    t="$(mktemp -d)"
    (
        set -e
        cd "$t"
        git init -q .
        mkdir -p ws_c/src/p ws_rust/src/r third-party/v/src/p
        for d in ws_c ws_rust third-party/v; do : > "$d/.colcon_workspace"; done
        : > ws_c/src/p/CMakeLists.txt
        : > ws_rust/src/r/Cargo.toml
        : > third-party/v/src/p/CMakeLists.txt
        git add ws_c ws_rust third-party
    ) >/dev/null 2>&1 || { rm -rf "$t"; echo "check-workspace-probes: selftest setup failed" >&2; return 1; }
    got="$(list_roots "$t")" || { rm -rf "$t"; return 1; }
    rm -rf "$t"
    if [ "$got" != "ws_c" ]; then
        echo "check-workspace-probes: selftest FAILED: roots were [${got//$'\n'/ }], want [ws_c]" >&2
        return 1
    fi
    return 0
}

# The selftest builds a repository of its own; clear an inherited git
# environment first (a hook's GIT_DIR would point it at this one).
# shellcheck source=scripts/lib/git-hook-env.sh
source scripts/lib/git-hook-env.sh
nros_clear_inherited_git_env
selftest || exit 1

if [ "$#" -gt 0 ]; then
    roots=("$@")
else
    mapfile -t roots < <(list_roots "$repo_root")
    wait $! || { echo "check-workspace-probes: could not list the workspace roots" >&2; exit 1; }
fi

logs="${NROS_GATE_SCRATCH:-$repo_root/tmp}/workspace-probes"
mkdir -p "$logs"
rm -f "$logs"/*.verdict
# Roots configure independently, so they run side by side: 53 roots took
# 1,687 s one after another on a warm host. NROS_WORKSPACE_PROBE_JOBS caps it.
jobs="${NROS_WORKSPACE_PROBE_JOBS:-6}"
start=$SECONDS

probe_root() {
    local root="$1" log t0=$SECONDS
    log="$logs/$(printf '%s' "$root" | tr '/' '_').log"
    if (cd "$root" && NROS_PROBE_FAILURE=refuse "$cli" configure) > "$log" 2>&1; then
        echo "ok $(( SECONDS - t0 )) $root" > "$log.verdict"
    else
        echo "FAILED $(( SECONDS - t0 )) $root" > "$log.verdict"
    fi
}

running=0
for root in "${roots[@]}"; do
    probe_root "$root" &
    running=$((running + 1))
    if [ "$running" -ge "$jobs" ]; then
        wait -n || true
        running=$((running - 1))
    fi
done
wait || true

failed=()
for root in "${roots[@]}"; do
    log="$logs/$(printf '%s' "$root" | tr '/' '_').log"
    if ! read -r verdict secs _ < "$log.verdict" 2>/dev/null; then
        verdict=FAILED secs=?
        echo "(no verdict recorded)" >> "$log"
    fi
    if [ "$verdict" = ok ]; then
        echo "  ok     ${secs}s  $root"
    else
        echo "  FAILED ${secs}s  $root" >&2
        tail -15 "$log" | sed 's/^/           /' >&2
        failed+=("$root")
    fi
done

echo ""
if [ "${#failed[@]}" -eq 0 ]; then
    echo "check-workspace-probes: ${#roots[@]} workspace(s) configure under refuse ($(( SECONDS - start ))s)"
    exit 0
fi
echo "check-workspace-probes: ${#failed[@]} of ${#roots[@]} workspace(s) refused a failed metadata probe:" >&2
printf '  %s\n' "${failed[@]}" >&2
echo "" >&2
echo "  The host probe compiles each C/C++ component to read what it declares." >&2
echo "  A component that only builds for its board (RTOS headers, devicetree," >&2
echo "  board registers) keeps that code in a source its CMakeLists.txt adds only" >&2
echo "  for that board, and gives the probe a host one. Logs: $logs" >&2
exit 1
