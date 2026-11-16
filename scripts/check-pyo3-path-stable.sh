#!/usr/bin/env bash
# check-pyo3-path-stable -- issue 2660: a PATH change does not rebuild pyo3.
#
# pyo3-build-config registers `rerun-if-env-changed=PATH` whenever
# PYO3_PYTHON is unset, so any PATH difference made cargo rebuild pyo3-ffi and
# everything above it (the launch resolver: ~30 s on CI). The recipes that
# build pyo3 crates now pin PYO3_PYTHON through
# `scripts/build/pyo3-python.sh`. This gate builds pyo3-ffi from the
# resolver's own workspace and lock, in a scratch target dir, and checks:
#
#   selftest PYO3_PYTHON unset, PATH prepended: cargo reports
#            `Dirty pyo3-ffi ...: the env variable PATH changed`. The trigger is
#            real, so the pinned case below can fail.
#   pinned   the helper's pin, PATH prepended: pyo3-ffi is `Fresh`.
#
# Builds only pyo3-build-config, target-lexicon and pyo3-ffi (a few seconds).
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$repo/packages/cli/nros-launch-resolve/Cargo.toml"

fail() { echo "check-pyo3-path-stable: FAIL -- $*" >&2; exit 1; }
# shellcheck source=scripts/lib/grep-q.sh
source "$repo/scripts/lib/grep-q.sh"   # nros_grep_q

[ -f "$manifest" ] || fail "no resolver manifest at $manifest"

mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/check-pyo3-path-stable.XXXXXX")"
trap '[ -n "${KEEP:-}" ] || rm -rf "$work"' EXIT
mkdir -p "$work/extra-path"
unset CARGO_BUILD_TARGET_DIR CARGO_BUILD_BUILD_DIR CARGO_BUILD_TARGET
export CARGO_TARGET_DIR="$work/target"

build() {  # build <log> -- one verbose pyo3-ffi build, output to <log>
    # profile-literal-ok: host tool: the resolver's pyo3-ffi, as setup-launch-resolve builds it
    cargo build -v --release --locked --manifest-path "$manifest" -p pyo3-ffi >"$1" 2>&1 \
        || { cat "$1" >&2; fail "cargo build -p pyo3-ffi failed (log above)"; }
}

# has <pattern> <log> -- 0 match, 1 none; a grep ERROR fails the gate.
has() {
    local rc=0
    nros_grep_q "$1" "$2" || rc=$?
    case "$rc" in
        0|1) return "$rc" ;;
        *) fail "grep failed (exit $rc) reading $2" ;;
    esac
}

base_path="$PATH"

# selftest -- the negative control, run on every invocation: PYO3_PYTHON
# unset and a PATH entry prepended must dirty pyo3-ffi, or the pinned check
# below could never fail.
selftest_unpinned_path_dirties() {
    (
        unset PYO3_PYTHON
        build "$work/control-1.log"
        PATH="$work/extra-path:$base_path" build "$work/control-2.log"
    )
    if ! has 'Dirty pyo3-ffi .*the env variable PATH changed' "$work/control-2.log"; then
        cat "$work/control-2.log" >&2
        fail "control: unpinned pyo3-ffi was not dirtied by a PATH change; this gate cannot fail"
    fi
}
selftest_unpinned_path_dirties

# pinned: the helper every pyo3 recipe sources.
# shellcheck source=scripts/build/pyo3-python.sh
source "$repo/scripts/build/pyo3-python.sh"
unset PYO3_PYTHON VIRTUAL_ENV CONDA_PREFIX
nros_pin_pyo3_python
[ -n "${PYO3_PYTHON:-}" ] || fail "nros_pin_pyo3_python found no Python 3 on PATH"
case "$PYO3_PYTHON" in /*) ;; *) fail "PYO3_PYTHON is not absolute: $PYO3_PYTHON" ;; esac
build "$work/pinned-1.log"
PATH="$work/extra-path:$base_path" build "$work/pinned-2.log"
if has 'Dirty pyo3-ffi' "$work/pinned-2.log" \
    || ! has 'Fresh pyo3-ffi' "$work/pinned-2.log"; then
    cat "$work/pinned-2.log" >&2
    fail "pinned (PYO3_PYTHON=$PYO3_PYTHON): a PATH change still rebuilt pyo3-ffi"
fi
echo "check-pyo3-path-stable: OK (PYO3_PYTHON=$PYO3_PYTHON; PATH change leaves pyo3-ffi Fresh, unpinned control Dirty)"
