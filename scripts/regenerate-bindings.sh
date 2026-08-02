#!/usr/bin/env bash
set -e

source scripts/build/cargo.sh
source scripts/build/generate-rust-incremental.sh

NROS="$(nros_cli_bin)"
# `nros sync` resolves the nano-ros runtime path-deps via NROS_REPO_DIR.
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
#     node pkg. Materialised with `nros sync` (codegen + writes the patch
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
# colcon shape `nros sync` shares one root `generated/` across) — exactly the
# `ws.rs` colcon heuristic. A standalone example carries its `package.xml` at the
# cargo root (no `src/<member>/package.xml`) and owns a per-package `generated/`.
ws_roots=()
while IFS= read -r member_pxml; do
    member_dir="$(dirname "$member_pxml")"  # <root>/src/<member>
    src_dir="$(dirname "$member_dir")"       # <root>/src
    [ "$(basename "$src_dir")" = "src" ] || continue
    root="$(dirname "$src_dir")"             # <root>
    # NO `[ -f "$root/Cargo.toml" ]` test. The comment above already says the
    # distinction is LAYOUT and names ws.rs's rule — "colcon-mode iff <root>/src/
    # exists AND at least one immediate subdir contains package.xml" — but the
    # code carried a second, narrower rule, and 19 colcon roots have no root
    # cargo manifest (every C/C++ workspace, six templates). Those fell through
    # to the standalone loop below, which generated a PER-LEAF `generated/` that
    # `nros sync` neither writes nor reads: the fossils dated 2026-05-24 under
    # `templates/{multi-package-workspace,local-msg-package}` came from exactly
    # this path, and they masked the wrong dep spelling by making it resolve on
    # a developer tree while failing on a fresh clone.
    ws_roots+=("$(cd "$root" && pwd)")
# `git ls-files`, NOT `find`. A `package.xml` is tracked, so this is an index
# lookup rather than a filesystem walk, and the difference is not marginal:
# measured on a built tree, the pruned `find` this replaces took **7m36s** to
# return the same 232 paths `git ls-files` returns in **0.8s**. It burned 0%
# CPU the whole time — pure I/O starvation, and this script ran that scan three
# times, which was the bulk of a two-hour fixture build.
#
# Pruning did not save it. `find` must still stat every directory on the way to
# deciding whether to prune it, so `-prune` cuts the descent but not the walk.
# The comment that used to sit here claimed pruning made the scan fast; it did
# not, and that claim is why the cost went unexamined for so long.
#
# Rule: never `find` for a file git tracks. Artifact scans (*.o, built ELFs,
# `target/` dirs being deleted) still need `find` — git cannot see untracked
# files — but those must be scoped to a build dir, not to `examples/`.
#
# ONE behaviour change, deliberate: a brand-new example whose `package.xml` is
# not yet `git add`ed is no longer discovered. That is a visible failure (its
# bindings simply do not generate) rather than a silent one, and staging a new
# file before building it is already how everything else in this repo behaves.
# It also drops the untracked `package.xml` COPIES that live under staged
# fixture dirs, which the old scan reached whenever a `build*` prune missed
# them.
# Two depths: `examples/<ws>/src/<member>` and `examples/templates/<t>/src/<member>`.
# The single-depth glob silently skipped every template workspace.
done < <(git ls-files 'examples/*/src/*/package.xml' 'examples/*/*/src/*/package.xml')
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

# --- 1. workspaces: ONE shared `generated/` at the root via `nros sync` ---
for root in "${ws_roots[@]}"; do
    # Only sync a workspace that actually declares message deps; a deps-less
    # workspace (e.g. the topic-forward bridge examples) has nothing to
    # materialise and `nros sync` would error on it.
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
    "$NROS" sync "$root" >/dev/null
done

# --- 2. standalone examples: per-package `generated/` (skip workspace members) ---
for pkg in $(git ls-files 'examples/**/package.xml' | sort); do
    dir="$(dirname "$pkg")"
    is_workspace_member "$dir" && continue
    nros_generate_rust_if_needed "$dir" "$NROS"
done

# --- 3. standalone test-bin pkgs (nros-bench / nros-tests/bins / nros-smoke) ---
for pkg in $(git ls-files \
        'packages/testing/nros-bench/**/package.xml' \
        'packages/testing/nros-tests/bins/**/package.xml' \
        'packages/testing/nros-smoke/**/package.xml' | sort); do
    dir="$(dirname "$pkg")"
    nros_generate_rust_if_needed "$dir" "$NROS"
done

echo "All bindings refreshed!"
