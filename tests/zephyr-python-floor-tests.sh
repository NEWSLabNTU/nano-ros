#!/usr/bin/env bash
# Issue 2821 — the Zephyr interpreter resolver honours the line's Python floor.
#
# Drives `nros_zephyr_python` (scripts/build/zephyr-python.sh) against synthetic
# workspaces whose `zephyr/cmake/modules/python.cmake` states a floor, with
# interpreters that are wrappers around this host's python3 and a stub `west`
# package on their path. No real west, no Zephyr tree. Buildless, tmpdir, <1 s.
#
# The negative control is row 3: a floor no interpreter meets must resolve to
# NOTHING. Before the floor was asked, the resolver took a python3 that imported
# west whatever its version, and the 4.4 line configured under 3.10.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

host_py="$(command -v python3)"
pass=0
fail=0
check() {
    if [ "$2" = "$3" ]; then
        pass=$((pass + 1)); echo "  ok    $1"
    else
        fail=$((fail + 1)); echo "  FAIL  $1: expected '$3', got '$2'"
    fi
}

# A stub `west` package, importable by every wrapper below.
mkdir -p "$tmp/stubs/west"
: > "$tmp/stubs/west/__init__.py"

# make_python <path>: an interpreter that can import west.
make_python() {
    mkdir -p "$(dirname "$1")"
    printf '#!/usr/bin/env bash\nPYTHONPATH="%s" exec "%s" "$@"\n' "$tmp/stubs" "$host_py" > "$1"
    chmod +x "$1"
}

# make_ws <dir> <floor>: a workspace whose tree states <floor>.
make_ws() {
    mkdir -p "$1/zephyr/cmake/modules"
    printf '# comment\nset(PYTHON_MINIMUM_REQUIRED %s)\n' "$2" > "$1/zephyr/cmake/modules/python.cmake"
}

# PATH's python3 is a west-capable wrapper, so the fallback candidate is known.
make_python "$tmp/pathbin/python3"
resolve() { (
    unset NROS_PYTHON
    PATH="$tmp/pathbin:/usr/bin:/bin"
    # shellcheck source=scripts/build/zephyr-python.sh
    source "$repo_root/scripts/build/zephyr-python.sh"
    "$@"
); }

# 1. The floor is READ from the tree, not authored.
make_ws "$tmp/ws-low" 3.0
check "floor read from python.cmake" "$(resolve nros_zephyr_python_floor "$tmp/ws-low")" "3.0"

# 2. A workspace venv comes first when it meets the floor.
make_ws "$tmp/ws-venv" 3.0
make_python "$tmp/ws-venv/.venv312/bin/python3"
check "workspace venv preferred" "$(resolve nros_zephyr_python "$tmp/ws-venv")" "$tmp/ws-venv/.venv312/bin/python3"

# 3. NEGATIVE CONTROL: a floor nothing meets resolves to nothing.
make_ws "$tmp/ws-high" 99.0
make_python "$tmp/ws-high/.venv312/bin/python3"
check "floor above every candidate resolves to nothing" "$(resolve nros_zephyr_python "$tmp/ws-high")" ""

# The in-repo venv sits between the workspace venv and PATH. It cannot be
# stubbed, so when this host has a usable one, it is the expected fallback.
fallback="$tmp/pathbin/python3"
repo_venv="$repo_root/scripts/zephyr/.venv/bin/python3"
if [ -x "$repo_venv" ] && "$repo_venv" -c 'import west' >/dev/null 2>&1; then
    fallback="$repo_venv"
fi

# 4. No workspace venv: the next candidate, when it meets the floor.
check "fallback when no workspace venv" "$(resolve nros_zephyr_python "$tmp/ws-low")" "$fallback"

# 5. NROS_PYTHON wins, even over the floor (it is the stated choice).
check "NROS_PYTHON wins" "$(NROS_PYTHON=/named/python bash -c 'source "$1" && nros_zephyr_python "$2"' _ "$repo_root/scripts/build/zephyr-python.sh" "$tmp/ws-high")" "/named/python"

# 6. A tree that states no floor imposes none.
mkdir -p "$tmp/ws-none/zephyr/cmake/modules"
: > "$tmp/ws-none/zephyr/cmake/modules/python.cmake"
check "no stated floor imposes none" "$(resolve nros_zephyr_python "$tmp/ws-none")" "$fallback"

echo "zephyr-python-floor: $pass passed, $fail failed"
[ "$fail" = 0 ]
