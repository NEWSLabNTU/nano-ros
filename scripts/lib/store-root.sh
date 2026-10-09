# shellcheck shell=bash
# store-root.sh — where the nano-ros store is (RFC-0103 D6, phase-484 W1).
#
# ONE variable names the store root: `$NROS_HOME`, else `$HOME/.nros`. Its
# categories (`sdk/`, `sources/`, `workspaces/`, `toolchains/`, `bin/`) are
# constructed under it. `NROS_STORE` and `NROS_SDK_STORE` are RETIRED — they
# answered the same question in two other orders, so one host could resolve two
# stores (issue 1767). A set one is refused, never silently ignored.
#
# The Rust twin is `nros_build_paths::store`; the cmake twin is
# `nros_store_root()` in `cmake/NanoRosStoreRoot.cmake`. Sourceable from any
# shell script; defines functions only.

# nros_store_retired — print the refusal for any retired variable that is set,
# return 1 if there was one. Silent and 0 otherwise.
nros_store_retired() {
    local bad=0
    if [ -n "${NROS_STORE+x}" ]; then
        echo "error: \$NROS_STORE is retired — set NROS_HOME to the same directory (RFC-0103 D6)" >&2
        bad=1
    fi
    if [ -n "${NROS_SDK_STORE+x}" ]; then
        echo "error: \$NROS_SDK_STORE is retired — set NROS_HOME to its PARENT (it named <store>/sdk; RFC-0103 D6)" >&2
        bad=1
    fi
    return "$bad"
}

# nros_store_root — print the store root. Fails (status 1, message on stderr)
# when a retired variable is set.
nros_store_root() {
    nros_store_retired || return 1
    printf '%s\n' "${NROS_HOME:-$HOME/.nros}"
}
