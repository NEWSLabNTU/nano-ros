#!/usr/bin/env bash

# Shared Cargo build knobs for broad build recipes.
#
# NROS_CARGO_PROFILE controls optimization profile. Use:
#   release          -> cargo build --release
#   nros-fast-release -> cargo build --profile nros-fast-release
#   dev              -> cargo build
#
# NROS_CARGO_FRONTENDS caps independent Cargo frontend processes. The
# compiler work inside each frontend still uses Cargo/rustc's native
# jobserver when MAKEFLAGS carries one.

nros_cargo_profile_name() {
    printf '%s\n' "${NROS_CARGO_PROFILE:-nros-fast-release}"
}

nros_cargo_profile_args() {
    local profile
    profile="$(nros_cargo_profile_name)"
    case "$profile" in
        dev)
            ;;
        release)
            printf '%s\n' "--release"
            ;;
        *)
            printf '%s\n' "--profile" "$profile"
            ;;
    esac
}

nros_cargo_nextest_args() {
    local profile
    profile="$(nros_cargo_profile_name)"
    case "$profile" in
        dev)
            ;;
        *)
            printf '%s\n' "--cargo-profile" "$profile"
            ;;
    esac
}

nros_cargo_profile_arg_string() {
    local profile
    profile="$(nros_cargo_profile_name)"
    case "$profile" in
        dev)
            ;;
        release)
            printf '%s\n' "--release"
            ;;
        *)
            printf '%s\n' "--profile $profile"
            ;;
    esac
}

nros_cargo_target_profile_dir() {
    local profile
    profile="$(nros_cargo_profile_name)"
    case "$profile" in
        dev)
            printf '%s\n' "debug"
            ;;
        release)
            printf '%s\n' "release"
            ;;
        *)
            printf '%s\n' "$profile"
            ;;
    esac
}

nros_cargo_frontend_jobs() {
    local jobs="${NROS_CARGO_FRONTENDS:-}"
    if [ -z "$jobs" ]; then
        if [ "${NROS_JOBSERVER:-}" = "1" ]; then
            jobs=4
        else
            jobs="${NROS_BUILD_JOBS:-8}"
        fi
    fi
    if ! [[ "$jobs" =~ ^[0-9]+$ ]] || [ "$jobs" -lt 1 ]; then
        echo "Invalid NROS_CARGO_FRONTENDS=$jobs; expected positive integer" >&2
        return 2
    fi
    printf '%s\n' "$jobs"
}

nros_cmake_frontend_jobs() {
    local jobs="${NROS_CMAKE_FRONTENDS:-}"
    if [ -z "$jobs" ]; then
        if [ "${NROS_JOBSERVER:-}" = "1" ]; then
            jobs=4
        else
            jobs="${NROS_BUILD_JOBS:-4}"
        fi
    fi
    if ! [[ "$jobs" =~ ^[0-9]+$ ]] || [ "$jobs" -lt 1 ]; then
        echo "Invalid NROS_CMAKE_FRONTENDS=$jobs; expected positive integer" >&2
        return 2
    fi
    printf '%s\n' "$jobs"
}

nros_cargo_fetch_root() {
    cargo fetch --locked
}

nros_cargo_fetch_codegen() {
    cargo fetch --locked --manifest-path packages/codegen/packages/Cargo.toml
}

nros_cli_bin() {
    if [ -n "${NROS_CLI:-}" ]; then
        if [ -x "$NROS_CLI" ]; then
            printf '%s\n' "$NROS_CLI"
            return 0
        fi
        echo "NROS_CLI points to a non-executable path: $NROS_CLI" >&2
        return 2
    fi
    if command -v nros >/dev/null 2>&1; then
        command -v nros
        return 0
    fi
    echo "nros CLI not found on PATH." >&2
    echo "Run: just setup base" >&2
    echo "Or set NROS_CLI=/path/to/nros." >&2
    return 2
}

# Phase 195.D: the codegen host tool is now the canonical `nros` binary
# (`nros codegen …`); the standalone `nros-codegen` (nros-codegen-c) was merged
# in. Function names keep `codegen_c` for callsite stability.
nros_cargo_codegen_c_bin() {
    printf '%s\n' "packages/codegen/packages/target/$(nros_cargo_target_profile_dir)/nros"
}

nros_cargo_build_codegen_c() {
    local cargo_profile_args
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo build $cargo_profile_args --manifest-path packages/codegen/packages/Cargo.toml \
        -p nros-cli --bin nros --quiet
}

nros_cargo_ensure_codegen_c() {
    local codegen_bin
    codegen_bin="$(nros_cargo_codegen_c_bin)"
    if [ "${NROS_CODEGEN_C_PREBUILT:-0}" = "1" ] && [ -x "$codegen_bin" ]; then
        return 0
    fi
    nros_cargo_build_codegen_c
}

nros_cargo_fetch_standalone_manifests() {
    local manifest
    local manifest_dir
    local list
    list="$(mktemp "${TMPDIR:-/tmp}/nros_cargo_fetch.XXXXXX")"
    trap 'rm -f "$list"' RETURN

    rg --files \
        examples \
        packages/testing/nros-tests/bins \
        packages/testing/nros-bench \
        -g Cargo.toml \
        -g '!examples/zephyr/**' \
        -g '!**/target/**' \
        -g '!**/generated/**' \
        -g '!**/build/**' \
        -g '!**/build-*/**' \
        -g '!**/_deps/**' \
        | sort > "$list"

    while IFS= read -r manifest; do
        manifest_dir="$(dirname "$manifest")"
        if [ -f "$manifest_dir/Cargo.lock" ]; then
            # No --locked: these are standalone examples/fixtures whose
            # Cargo.lock is gitignored (not reproducibility-critical), and a
            # clean+setup can leave them stale (deps shrank/bumped). `--locked`
            # made the prefetch hard-fail ("cannot update the lock file …")
            # instead of refreshing them; this prefetch is just cache-warming
            # for the offline fanout, so allow the lock to refresh here.
            ( cd "$manifest_dir" && cargo fetch --quiet )
        fi
    done < "$list"
}
