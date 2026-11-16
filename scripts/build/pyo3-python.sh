# shellcheck shell=bash
#
# Pin PYO3_PYTHON to the interpreter pyo3 would pick anyway, so a PATH change
# no longer rebuilds every pyo3 crate. Issue 2660.
#
# Sourced, not run: `source scripts/build/pyo3-python.sh; nros_pin_pyo3_python`.
#
# WHY. pyo3-build-config's `find_interpreter` (0.25 and 0.29 alike; the tree is on 0.29) searches
# `python`, then `python3`, on PATH, and before it does it prints
# `cargo:rerun-if-env-changed=PATH`. So a PATH that differs by ANY entry, even
# one with no Python in it, makes cargo report
#
#   Dirty pyo3-ffi v0.29.2: the env variable PATH changed
#
# and rebuilds pyo3-ffi, pyo3 and everything above them: about 30 s for the
# launch resolver on CI. A consumer that puts the `nros` CLI on PATH before
# calling `just setup-launch-resolve` paid that on every call. With
# PYO3_PYTHON set, pyo3 reads that variable and never registers PATH.
#
# WHAT IT PINS. The same interpreter pyo3 would have found, in pyo3's own
# order, resolved to an absolute path, so the build is unchanged and only the
# rebuild trigger moves from PATH to the interpreter itself:
#
#   1. PYO3_PYTHON already set: left alone (the caller chose).
#   2. Exactly one of VIRTUAL_ENV / CONDA_PREFIX set: left unset. pyo3 takes
#      that environment's interpreter and does not register PATH.
#   3. Otherwise the first of `python`, `python3` on PATH that reports
#      Python 3, exactly pyo3's test.
#   4. None: left unset, so pyo3 reports its own "no Python 3.x interpreter".
#
# Never exports an empty value: pyo3 treats a set-but-empty PYO3_PYTHON as an
# interpreter named "".
nros_pin_pyo3_python() {
    if [ -n "${PYO3_PYTHON:-}" ]; then
        return 0
    fi
    if [ -n "${VIRTUAL_ENV:-}" ] && [ -z "${CONDA_PREFIX:-}" ]; then
        return 0
    fi
    if [ -z "${VIRTUAL_ENV:-}" ] && [ -n "${CONDA_PREFIX:-}" ]; then
        return 0
    fi
    local _bin _path _ver
    for _bin in python python3; do
        _path="$(command -v "$_bin" 2>/dev/null)" || continue
        case "$_path" in /*) ;; *) continue ;; esac
        _ver="$("$_path" --version 2>&1)" || continue
        case "$_ver" in
            "Python 3"*|"GraalPy 3"*)
                export PYO3_PYTHON="$_path"
                return 0
                ;;
        esac
    done
    return 0
}
