#!/usr/bin/env bash
# phase-445 W4b (RFC-0098 D1) — build a single-package cargo leaf through its
# generated `build/<image>/nros-cargo.toml`, the same file `nros build` hands
# cargo. Sourced by `fixtures-build.sh` AND by its staleness probe
# (`scripts/test/rust-fixture-stale.sh`): the probe rebuilds a row with the
# row's EXACT invocation to ask cargo whether it is fresh, so a second spelling
# of the invocation here would be a permanent false-STALE (issue 0196's class).
#
# Why the invocation has this shape — both facts measured, see
# `packages/cli/nros-cli-core/src/cmd/leaf_settings.rs`:
#
#   * cargo resolves the file's relative paths against its GRANDPARENT
#     (`<leaf>/build/`), and the writer already writes them that way;
#   * cargo DISCOVERS `.cargo/config.toml` from the working directory, and the
#     leaf's own (deleted by phase-445 W6) repeats the board's link flags, which
#     cargo JOINS — `region 'FLASH' already defined` on mps2. So cargo runs from
#     the directory ABOVE the leaf, with the manifest and the settings NAMED.
#
# Every path here is absolute: the row's `dir` is repo-relative and the working
# directory is no longer the leaf.

# nros_leaf_settings_path <leaf-dir>
# The leaf's settings file as `nros ws leaf-system` reports it, or nothing when
# the leaf does not build through one. FAILS for a leaf that states its board
# in `system.toml` (the predicate `fixtures-manifest.py` uses to drop the row's
# `--target`) but reports no settings file — building it the old way would
# silently compile for the host.
#
# Needs `$NROS_CLI` (both callers export it) rather than calling
# `nros_cli_bin` itself: this runs in a make leaf, which has only the functions
# `export -f` gave it (`check-export-f-closure`).
nros_leaf_settings_path() {
    local dir="$1" root="${NROS_REPO_ROOT:-$PWD}" out path
    [ -f "$root/$dir/system.toml" ] && [ -f "$root/$dir/Cargo.toml" ] || return 0
    if [ -z "${NROS_CLI:-}" ]; then
        printf 'nros_leaf_settings_path: NROS_CLI is not set\n' >&2
        return 1
    fi
    out="$(NROS_REPO_DIR="$root" "$NROS_CLI" ws leaf-system "$root/$dir" 2>&1)" || {
        printf 'nros_leaf_settings_path: "nros ws leaf-system %s" failed:\n%s\n' "$dir" "$out" >&2
        return 1
    }
    path="$(printf '%s\n' "$out" | sed -n 's/^NROS_LEAF_SETTINGS=//p')"
    if [ -z "$path" ]; then
        printf 'nros_leaf_settings_path: %s declares system.toml but reports no NROS_LEAF_SETTINGS\n' "$dir" >&2
        return 1
    fi
    if [ ! -f "$path" ]; then
        printf 'nros_leaf_settings_path: %s is missing — run "nros sync %s" first (RFC-0098 D2)\n' \
            "$path" "$dir" >&2
        return 1
    fi
    printf '%s' "$path"
}

# nros_leaf_settings_cwd <leaf-dir>
# Where cargo runs for a settings-file row: the directory above the leaf.
nros_leaf_settings_cwd() {
    local root="${NROS_REPO_ROOT:-$PWD}"
    dirname "$root/$1"
}

# nros_leaf_settings_args <leaf-dir> <settings-path>
# `--manifest-path … --config …`, both absolute.
nros_leaf_settings_args() {
    local root="${NROS_REPO_ROOT:-$PWD}"
    printf -- '--manifest-path %s --config %s' "$root/$1/Cargo.toml" "$2"
}

# nros_leaf_settings_target_dir_flag <leaf-dir> <cargo-args>
# For a row with NO shared group dir. The settings file names a per-image
# target dir under `build/`, but the test resolver finds a group-less row's
# artifacts at `<leaf>/<authored target_dir or target>` (`row_artifact_root`),
# and that path must not move. An authored `--target-dir` was relative to the
# leaf, which is no longer the working directory, so it is made absolute here.
nros_leaf_settings_target_dir_flag() {
    local dir="$1" root="${NROS_REPO_ROOT:-$PWD}" td="" prev=""
    local tok
    # shellcheck disable=SC2086
    for tok in $2; do
        if [ "$prev" = "--target-dir" ]; then td="$tok"; fi
        prev="$tok"
    done
    case "$td" in
        "") td="$root/$dir/target" ;;
        /*) ;;
        *) td="$root/$dir/$td" ;;
    esac
    printf ' --target-dir %s' "$td"
}
