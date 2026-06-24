#!/usr/bin/env bash
set -e

source scripts/build/cargo.sh
source scripts/build/generate-rust-incremental.sh

NROS="$(nros_cli_bin)"
# `nros ws sync` resolves the nano-ros runtime path-deps via NROS_REPO_DIR.
export NROS_REPO_DIR="${NROS_REPO_DIR:-$PWD}"
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
#     Materialised with `nros generate-rust` (codegen only; the patch block ships).
#
#   * Cargo workspace      → ONE SHARED `generated/` at the workspace root,
#     resolved by the workspace manifest's `[patch.crates-io]` for every member
#     node pkg. Materialised with `nros ws sync` (codegen + writes the patch
#     block, which is NOT committed). Member pkgs must NOT each get a per-package
#     `generated/` — that is redundant + unreferenced.
#
# Bindings are build-time + ROS-version-dependent, so `generated/` is gitignored
# (content depends on the ROS msg pkg versions on the build host) — never shipped.

# --- discover MULTI-PACKAGE workspace roots (colcon layout) ---
# The distinction is LAYOUT, not `[workspace]`: standalone copy-out examples ALSO
# carry an (empty) `[workspace]` table to stop cargo walking up to the repo root
# (CLAUDE.md "Examples are standalone copy-out projects; no workspace walk-up"). A
# multi-package workspace is a dir whose `src/<member>/package.xml` exists (the
# colcon shape `nros ws sync` shares one root `generated/` across) — exactly the
# `ws.rs` colcon heuristic. A standalone example carries its `package.xml` at the
# cargo root (no `src/<member>/package.xml`) and owns a per-package `generated/`.
ws_roots=()
while IFS= read -r member_pxml; do
    member_dir="$(dirname "$member_pxml")"  # <root>/src/<member>
    src_dir="$(dirname "$member_dir")"       # <root>/src
    [ "$(basename "$src_dir")" = "src" ] || continue
    root="$(dirname "$src_dir")"             # <root>
    [ -f "$root/Cargo.toml" ] || continue
    ws_roots+=("$(cd "$root" && pwd)")
# PRUNE the heavy build trees (`target/`, `generated/`, `build*/` incl the
# vendored `_deps/` under cmake build dirs) — `-not -path` only FILTERS, find still
# descends into them, which over a built example tree takes many minutes.
done < <(find examples \
    \( -name target -o -name generated -o -name 'build*' \) -prune -o \
    -path '*/src/*/package.xml' -print 2>/dev/null)
# de-duplicate (one entry per workspace, not per member)
if [ "${#ws_roots[@]}" -gt 0 ]; then
    mapfile -t ws_roots < <(printf '%s\n' "${ws_roots[@]}" | sort -u)
fi

is_workspace_member() {
    # true iff $1 is at or below any discovered workspace root
    local dir
    dir="$(cd "$1" && pwd)"
    local root
    for root in "${ws_roots[@]}"; do
        case "$dir/" in "$root"/*) return 0 ;; esac
    done
    return 1
}

# --- 1. workspaces: ONE shared `generated/` at the root via `nros ws sync` ---
for root in "${ws_roots[@]}"; do
    # Only sync a workspace that actually declares message deps; a deps-less
    # workspace (e.g. the topic-forward bridge examples) has nothing to
    # materialise and `nros ws sync` would error on it.
    member_deps="$(find "$root" \( -name target -o -name generated -o -name 'build*' \) -prune -o \
        -name package.xml -print 2>/dev/null \
        | xargs -r grep -lE '<(depend|exec_depend|build_depend)>' 2>/dev/null | wc -l)"
    [ "$member_deps" -gt 0 ] || continue
    # Drop any stale PER-MEMBER `generated/` left by an older per-pkg pass; it is
    # gitignored + unreferenced (the shared root `generated/` is the source of truth).
    if [ -d "$root/src" ]; then
        find "$root/src" -mindepth 2 -maxdepth 2 -type d -name generated -exec rm -rf {} + 2>/dev/null || true
    fi
    echo "  ws sync: ${root#"$PWD"/}"
    "$NROS" sync "$root" >/dev/null
done

# --- 2. standalone examples: per-package `generated/` (skip workspace members) ---
for pkg in $(find examples \
        \( -name target -o -name generated -o -name 'build*' \) -prune -o \
        -name package.xml -print 2>/dev/null | sort); do
    dir="$(dirname "$pkg")"
    is_workspace_member "$dir" && continue
    nros_generate_rust_if_needed "$dir" "$NROS"
done

# --- 3. standalone test-bin pkgs (nros-bench / nros-tests/bins / nros-smoke) ---
for pkg in $(find packages/testing/nros-bench packages/testing/nros-tests/bins packages/testing/nros-smoke \
                 \( -name target -o -name generated -o -name 'build*' \) -prune -o \
                 -name package.xml -print 2>/dev/null | sort); do
    dir="$(dirname "$pkg")"
    nros_generate_rust_if_needed "$dir" "$NROS"
done

echo "All bindings refreshed!"
