#!/usr/bin/env bash
# Shared fixture matrix primitives. Keep this file shell-only so every
# platform just recipe can source it without pulling in Python or Rust.

nros_fixture_roles() {
    printf '%s\n' \
        talker \
        listener \
        service-server \
        service-client \
        action-server \
        action-client
}

nros_fixture_langs() {
    printf '%s\n' rust c cpp
}

nros_zephyr_lang_tag() {
    case "$1" in
        rust) printf '%s\n' rs ;;
        c) printf '%s\n' c ;;
        cpp) printf '%s\n' cpp ;;
        *)
            echo "unknown fixture language: $1" >&2
            return 2
            ;;
    esac
}

# Phase 177.9 — content-hash staleness signatures for prebuilt fixtures.
#
# The CMake identity `signature` above only gates *reconfigure*; cargo/ninja
# handle incremental *rebuilds* during a `build-fixtures` run. But the test
# harness consumes PREBUILT binaries — if a source is edited and
# `build-fixtures` is never re-run, the harness silently uses a stale binary.
# `nros_fixture_cell_sig` hashes the inputs that feed a cell so the `test-all`
# preflight can flag that. Shell-only by design (see file header).
#
# Shared inputs (affect every cell): the workspace crates, the lockfile, the
# Rust toolchain pin, the CMake glue, and the third-party SDK submodule pins.
# Hashed once per process; the orchestrator may export NROS_FIXTURE_SHARED_SIG
# so parallel children skip the recompute.
nros_fixture_shared_sig() {
    if [ -n "${NROS_FIXTURE_SHARED_SIG:-}" ]; then
        printf '%s\n' "$NROS_FIXTURE_SHARED_SIG"
        return 0
    fi
    if [ -n "${_NROS_FIXTURE_SHARED_SIG_CACHE:-}" ]; then
        printf '%s\n' "$_NROS_FIXTURE_SHARED_SIG_CACHE"
        return 0
    fi
    local sig
    sig="$(
        {
            git ls-files packages Cargo.lock rust-toolchain.toml cmake 2>/dev/null \
                | tr '\n' '\0' | xargs -0 sha1sum 2>/dev/null
            # Submodule gitlinks (mode 160000 + recorded commit) capture SDK pins.
            git ls-files -s third-party 2>/dev/null
            cat .gitmodules 2>/dev/null
        } | sha1sum | awk '{ print $1 }'
    )"
    _NROS_FIXTURE_SHARED_SIG_CACHE="$sig"
    printf '%s\n' "$sig"
}

# nros_fixture_cell_sig <example-src-dir> — hash(shared inputs + the cell's own
# tracked sources). Tracked-only (git ls-files), so generated/build artifacts
# are excluded. Deterministic: the build writer and the preflight reader
# compute the same value from the same tree.
nros_fixture_cell_sig() {
    local src_dir="$1"
    local shared
    shared="$(nros_fixture_shared_sig)"
    {
        printf 'shared=%s\n' "$shared"
        git ls-files "$src_dir" 2>/dev/null | tr '\n' '\0' | xargs -0 sha1sum 2>/dev/null
    } | sha1sum | awk '{ print $1 }'
}

# Make the signature helpers available inside `parallel`/`bash -c` children
# that callers spawn after `export -f nros_cmake_fixture_build`.
export -f nros_fixture_shared_sig nros_fixture_cell_sig 2>/dev/null || true

nros_cmake_fixture_build() {
    local src_dir="$1"
    local build_dir="$2"
    # $3 (the old identity signature) is accepted for caller compatibility but
    # unused: per-RMW build dirs have fixed args, so there is no arg-change
    # reconfigure to track; `cmake --build` auto-reconfigures on CMakeLists /
    # dependency-graph changes (Phase 181.7b).
    shift 3

    # Prefer Ninja when available; fall back to CMake's default generator.
    local gen=()
    local want_gen="default"
    if command -v ninja >/dev/null 2>&1; then
        gen=(-G Ninja)
        want_gen="Ninja"
    fi

    # Wipe a dir configured with a different generator so the switch reconfigures.
    if [ -f "$build_dir/CMakeCache.txt" ]; then
        local cur_gen
        cur_gen="$(sed -n 's/^CMAKE_GENERATOR:INTERNAL=//p' "$build_dir/CMakeCache.txt")"
        if { [ "$want_gen" = "Ninja" ] && [ "$cur_gen" != "Ninja" ]; } || \
           { [ "$want_gen" = "default" ] && [ "$cur_gen" = "Ninja" ]; }; then
            rm -rf "$build_dir"
        fi
    fi

    # Configure once: missing cache, or a cache with no generated build system
    # (a previously-failed configure). `cmake --build` then handles reconfigure.
    if [ ! -f "$build_dir/CMakeCache.txt" ] || \
       { [ ! -f "$build_dir/build.ninja" ] && [ ! -f "$build_dir/Makefile" ]; }; then
        if ! cmake -S "$src_dir" -B "$build_dir" "${gen[@]}" "$@"; then
            return 1
        fi
    fi
    cmake --build "$build_dir"
}
