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

nros_nextest_profile_args() {
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
