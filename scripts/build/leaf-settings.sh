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

# nros_leaf_settings_path <leaf-dir> [<image>]
# The leaf's settings file as `nros ws leaf-system` reports it, or nothing when
# the leaf does not build through one. FAILS for a leaf that states its board
# in `system.toml` (the predicate `fixtures-manifest.py` uses to drop the row's
# `--target`) but reports no settings file — building it the old way would
# silently compile for the host.
#
# Issue 1305 — it also WRITES the image's selection package and settings file
# (`nros ws leaf-system --write-settings`, the writer `nros build` uses), for
# `<image>` when the row names one (`--nros-image=<id>` in its args) and for
# the leaf's default image otherwise. A cargo leaf with several images has ONE
# `build/nros/selection` for all of them, so the row that is about to build
# must put its own image there — call this inside `nros_leaf_lock`, which is
# what keeps a sibling row from rewriting it before cargo has read it.
#
# Needs `$NROS_CLI` (both callers export it) rather than calling
# `nros_cli_bin` itself: this runs in a make leaf, which has only the functions
# `export -f` gave it (`check-export-f-closure`).
nros_leaf_settings_path() {
    local dir="$1" image="${2:-}" root="${NROS_REPO_ROOT:-$PWD}" out path
    [ -f "$root/$dir/system.toml" ] && [ -f "$root/$dir/Cargo.toml" ] || return 0
    if [ -z "${NROS_CLI:-}" ]; then
        printf 'nros_leaf_settings_path: NROS_CLI is not set\n' >&2
        return 1
    fi
    local image_args=()
    [ -z "$image" ] || image_args=(--image "$image")
    out="$(NROS_REPO_DIR="$root" "$NROS_CLI" ws leaf-system --write-settings \
        "${image_args[@]}" "$root/$dir" 2>&1)" || {
        printf 'nros_leaf_settings_path: "nros ws leaf-system --write-settings %s %s" failed:\n%s\n' \
            "${image_args[*]}" "$dir" "$out" >&2
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

# ── issue 1305 — one leaf, several images ─────────────────────────────────
#
# A `[[fixture]]` row of a single-package leaf names its image as one token in
# its cargo args, `--nros-image=<id>` (`fixtures-manifest.py`, `ROW_IMAGE_TOKEN`)
# — in the args because that string is the row's group key, so two images land
# in two artifact dirs by the computation that already separates two feature
# sets. These are its only readers.

# nros_leaf_row_image <cargo-args>
# The image the row names, or nothing.
nros_leaf_row_image() {
    local tok
    # shellcheck disable=SC2086
    for tok in ${1:-}; do
        case "$tok" in
            --nros-image=*) printf '%s' "${tok#--nros-image=}" ;;
        esac
    done
}

# nros_leaf_strip_row_image <cargo-args>
# The args cargo is given: the row's, minus the image token.
nros_leaf_strip_row_image() {
    local tok out=()
    # shellcheck disable=SC2086
    for tok in ${1:-}; do
        case "$tok" in
            --nros-image=*) ;;
            *) out+=("$tok") ;;
        esac
    done
    printf '%s' "${out[*]:-}"
}

# nros_leaf_lock <leaf-dir> <command> [args…]
# Run <command> holding the leaf's exclusive lock.
#
# Every image of a leaf shares `<leaf>/build/nros/selection` and
# `<leaf>/generated/`, and the fixture build runs rows in parallel (make -j), so
# two rows of one leaf naming different images would each rewrite the
# selection the other's cargo is compiling against — an artifact built with the
# wrong backend, in the right directory. Rows of DIFFERENT leaves still run in
# parallel; rows of one leaf take turns, each writing its selection and
# building before the next. Measured: the six native leaves have 3-5 rows each.
# The staleness probe takes the same lock, because it rebuilds a row too.
nros_leaf_lock() {
    local dir="$1" root="${NROS_REPO_ROOT:-$PWD}"
    shift
    if ! command -v flock >/dev/null 2>&1; then
        printf 'nros_leaf_lock: `flock` (util-linux) is required to serialise the rows of %s\n' \
            "$dir" >&2
        return 1
    fi
    mkdir -p "$root/$dir/build/nros"
    (
        flock -x 9
        "$@"
    ) 9>"$root/$dir/build/nros/.leaf.lock"
}
