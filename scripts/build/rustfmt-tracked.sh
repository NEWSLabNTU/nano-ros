#!/usr/bin/env bash
# rustfmt over TRACKED Rust sources, one call per crate, with no cargo
# metadata (issue 2464).
#
# Usage: scripts/build/rustfmt-tracked.sh <examples|cli> [--check]
#   NIGHTLY  the rustfmt toolchain (`just` passes its own `NIGHTLY`; default:
#            the channel in tools/rust-toolchain.toml)
#
# Formatting reads source files and needs no dependency graph. `cargo fmt`
# loads the manifest anyway, so it needs whatever the manifest names to
# exist: the gitignored `nros-patch.toml` include, the `generated/` path deps
# that `nros configure` and codegen produce, PX4's `px4_msgs` (issue 1114), and
# for `packages/cli` a git-tag dep fetched from the network (issue 0337). The
# `--check` gates moved to `rustfmt` per file long ago (issues 0320 and
# 0337), and the formatters stayed on `cargo fmt`. So a fresh worktree could
# pass `check fast` and still fail `just format` at four different steps.
# Both now come here. A gate and its formatter enumerate the same files with
# the same edition, so `--check` passes on whatever the formatter wrote.
#
# A second effect: formatting tracked files only, it never touches a leaf's
# `generated/` members. `cargo fmt` formatted them, which re-staled every
# native fixture (CLAUDE.md, "Fixture mtime treadmill").
#
# The sets:
#   examples  every example leaf (a `Cargo.toml` at depth 5 or more), minus
#             the platforms whose leaves are built by their own lanes and the
#             PX4 companions, whose generated `px4_msgs` come only from
#             `just setup px4`.
#   cli       the members of the `packages/cli` workspace, read from its
#             manifest. Test fixture crates under `tests/fixtures/` are separate
#             packages, and several are deliberately on older editions.
set -euo pipefail

usage() {
    echo "usage: $0 <examples|cli> [--check]" >&2
    exit 2
}

[ "$#" -ge 1 ] || usage
set_name="$1"
shift
check=()
case "${1:-}" in
    --check) check=(--check) ;;
    "") ;;
    *) usage ;;
esac

root="$(git rev-parse --show-toplevel)"
cd "$root"
nightly="${NIGHTLY:-$(awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml)}"

# A manifest's own `edition = "NNNN"`, else the fallback given.
edition_of() {
    local e
    e="$(sed -n 's/^edition[[:space:]]*=[[:space:]]*"\([0-9]*\)".*/\1/p' "$1" | head -1)"
    echo "${e:-$2}"
}

# One crate: its directory, its edition, and the tracked files to leave out.
# Tracked files only, through the git index: build and target trees and
# every `generated/` tree are untracked, so none can leak in.
fmt_crate() {
    local dir="$1" edition="$2" exclude="${3:-}"
    local listed files
    # An assignment, not a process substitution, so a failing `git ls-files`
    # stops the script under `set -e` rather than reading as no files.
    listed="$(git ls-files "$dir/*.rs" "$dir/**/*.rs")"
    if [ -n "$exclude" ]; then
        listed="$(grep -v -- "$exclude" <<<"$listed" || true)"
    fi
    [ -z "$listed" ] && return 0
    mapfile -t files <<<"$listed"
    echo "  fmt $dir (edition $edition, ${#files[@]} files)"
    rustfmt "+$nightly" "${check[@]}" --edition "$edition" "${files[@]}"
}

case "$set_name" in
    examples)
        # `NF>=5`: leaf crates, not workspace-root manifests.
        git ls-files 'examples/**/Cargo.toml' | awk -F/ 'NF>=5' \
            | grep -vE '/(target|generated|build|build-[^/]*|install|log)/|examples/zephyr/|/multi-package-workspace/|esp32-c3-baremetal/rust/dds/|examples/mps2-an385-freertos/|examples/qemu-armv7a-nuttx/|examples/threadx-linux/|examples/rv-virt-threadx/|examples/px4/' \
            | sort | while read -r toml; do
                # The tree is not on one edition: read it, never assume it.
                fmt_crate "$(dirname "$toml")" "$(edition_of "$toml" 2024)"
            done
        ;;
    cli)
        ws_edition="$(edition_of packages/cli/Cargo.toml 2024)"
        members="$(sed -n '/^members = \[/,/^]/p' packages/cli/Cargo.toml \
            | sed -n 's/^[[:space:]]*"\([^"]*\)".*/\1/p')"
        for m in $members; do
            toml="packages/cli/$m/Cargo.toml"
            [ -f "$toml" ] || continue
            # `edition.workspace = true` inherits the workspace's.
            fmt_crate "packages/cli/$m" "$(edition_of "$toml" "$ws_edition")" '/tests/fixtures/'
        done
        ;;
    *) usage ;;
esac
