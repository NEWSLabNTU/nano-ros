#!/usr/bin/env bash
set -e

source scripts/build/cargo.sh
source scripts/build/generate-rust-incremental.sh

NROS="$(nros_cli_bin)"
# `nros configure` resolves the nano-ros runtime path-deps via NROS_REPO_DIR.
# Issue 1840 — an inherited value naming ANOTHER checkout is re-rooted onto
# this one by the one rule; an out-of-tree root is kept.
. scripts/lib/checkout-paths.sh
NROS_REPO_DIR="$(nros_reroot_checkout_path "${NROS_REPO_DIR:-$PWD}" "$PWD")"
export NROS_REPO_DIR
# Codegen only EMITS message structs — its output does not depend on the runtime
# `nros-core` ABI, so bypass the CLI-vs-workspace ABI guard (phase-265
# `abi_guard.rs`) here, exactly as `scripts/ci/dep-chain-check.sh` does for the same
# reason. (Some example Cargo.locks still resolve an older `nros-core`; that is a
# resolution concern for the build lanes, not for binding codegen.)
export NROS_SKIP_VERSION_CHECK=1
echo "Refreshing Rust bindings..."

# Two binding layouts (one per package shape — see RFC-0023 / RFC-0024):
#
#   * Standalone example  → a PER-PACKAGE `generated/` beside the pkg's
#     `package.xml`, resolved by that pkg's own (committed) `[patch.crates-io]`.
#     Materialised with `nros dev codegen rust` (codegen only; the patch block ships).
#
#   * Cargo workspace      → ONE SHARED `generated/` at the workspace root,
#     resolved by the workspace manifest's `[patch.crates-io]` for every member
#     node pkg. Materialised with `nros configure` (codegen + writes the patch
#     block, which is NOT committed). Member pkgs must NOT each get a per-package
#     `generated/` — that is redundant + unreferenced.
#
# Bindings are build-time + ROS-version-dependent, so `generated/` is gitignored
# (content depends on the ROS msg pkg versions on the build host) — never shipped.

# --- the codegen UNITS: one discovery, `scripts/lib/codegen_scope.py` ---
# Three kinds, each generated its own way below:
#
#   workspace   a MULTI-PACKAGE colcon root (`<root>/src/<member>/package.xml`),
#               ONE shared `generated/` via `nros dev bindings`. The distinction
#               is LAYOUT, not `[workspace]` (a standalone example also carries
#               an empty `[workspace]` to stop cargo walking up), and a colcon
#               root needs no root `Cargo.toml` (every C/C++ workspace and six
#               templates have none; generating them per-leaf left fossils that
#               masked a wrong dep spelling).
#   standalone  an example `package.xml` dir outside every colcon root, with a
#               per-package `generated/`.
#   testbin     nros-bench / nros-tests bins / nros-smoke packages.
#
# Discovery reads `git ls-files`, never `find`: the pruned `find` this once was
# took 7m36s on a built tree to return what the index returns in 0.8s (a
# tracked file is an index lookup; a brand-new `package.xml` must be `git add`ed
# before it is generated, which is a visible failure, not a silent one).
#
# phase-535 W10 (issue 2475 defect 4): `NROS_CODEGEN_COORDS=<lane coords file>`
# narrows the units to those the lane's fixture rows live in, so a one-board
# shard stops generating the whole tree. `just build <lane>` sets it through
# `_codegen <lane>`; unset (lane `all`, a bare `just generate-bindings`) means
# every unit, as before. The `_require-leaf-includes` guard reads the SAME
# selection, so it checks exactly what was generated here.
ws_roots=()
standalone_dirs=()
testbin_dirs=()
if ! units="$(python3 scripts/lib/codegen_scope.py units </dev/null)"; then
    echo "regenerate-bindings: could not list the codegen units" >&2
    exit 1
fi
while IFS=$'\t' read -r kind dir; do
    [ -n "$kind" ] || continue
    case "$kind" in
        workspace) ws_roots+=("$PWD/$dir") ;;
        standalone) standalone_dirs+=("$dir") ;;
        testbin) testbin_dirs+=("$dir") ;;
        *) echo "regenerate-bindings: unknown unit kind '$kind'" >&2; exit 1 ;;
    esac
done <<< "$units"

# --- 1. workspaces: ONE shared `generated/` at the root via `nros configure` ---
for root in "${ws_roots[@]}"; do
    # Only sync a workspace that actually declares message deps; a deps-less
    # workspace (e.g. the topic-forward bridge examples) has nothing to
    # materialise and `nros configure` would error on it.
    # `$root` is absolute (set via `cd && pwd`); git pathspecs are repo-relative.
    rel_root="${root#"$PWD"/}"
    member_deps="$(git ls-files "$rel_root" \
        | grep '/package\.xml$' \
        | xargs -r grep -lE '<(depend|exec_depend|build_depend)>' 2>/dev/null | wc -l)"
    [ "$member_deps" -gt 0 ] || continue
    # Drop any stale PER-MEMBER `generated/` left by an older per-pkg pass; it is
    # gitignored + unreferenced (the shared root `generated/` is the source of truth).
    #
    # A glob, not `find`. These dirs are gitignored, so `git ls-files` cannot see
    # them — but the path is a fixed shape (`<root>/src/<member>/generated`), and
    # a glob expands it without walking anything. `find` would descend through
    # every member's build output to rediscover a depth it was already told.
    for _gen in "$root"/src/*/generated; do
        if [ -d "$_gen" ]; then rm -rf "$_gen"; fi
    done
    echo "  sync: ${root#"$PWD"/}"
# phase-367 W5 — `--no-provider-index`: this driver never READS
# `<ws>/build/nros/providers.json`. cmake keeps its own index at
# `${CMAKE_BINARY_DIR}/nros-providers.json` and reads it THROUGH the CLI, and
# no caller points `nano_ros_load_providers(INDEX …)` at the sync-written one.
# Writing it costs the underlay scan — 28 %% of a warm sync after W1/W2
# (0.095 s -> 0.068 s), across ~101 syncs a build.
    "$NROS" dev bindings --no-provider-index "$root" >/dev/null
done

# --- 2. standalone examples: per-package `generated/` (workspace members excluded) ---
for dir in "${standalone_dirs[@]}"; do
    nros_generate_rust_if_needed "$dir" "$NROS"
done

# --- 3. standalone test-bin pkgs (nros-bench / nros-tests/bins / nros-smoke) ---
for dir in "${testbin_dirs[@]}"; do
    nros_generate_rust_if_needed "$dir" "$NROS"
done

echo "All bindings refreshed!"
