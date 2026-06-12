#!/usr/bin/env bash
# Build-stage "compile-check" fixtures (issue 0034 — No compilation inside tests).
#
# Some tests only need to prove that a small generated/template crate *compiles*
# (e.g. a macro re-export path resolves). Running `cargo check` inside the test
# makes the test wall-clock dominated by compile time → spurious nextest
# timeouts. Instead, this script does the compile in the BUILD stage: it stages
# each template into a gitignored build dir, rewrites `@NANO_ROS_ROOT@`
# placeholders to absolute `path =` deps, runs `cargo check`, and on success
# writes a `.compile-ok` stamp the test asserts (via
# `nros_tests::fixtures::require_compile_check`).
#
# Add an entry to COMPILE_CHECK_FIXTURES (id : template-dir relative to repo).
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cd "$repo_root"

# shellcheck source=scripts/build/cargo.sh
source "$repo_root/scripts/build/cargo.sh"

out_root="$repo_root/build/compile-check"
mkdir -p "$out_root"

# id : source template dir (carries @NANO_ROS_ROOT@ placeholders)
COMPILE_CHECK_FIXTURES=(
    "one_dep_component_pkg:packages/testing/nros-tests/fixtures/one_dep_component_pkg"
)

stage_and_check() {
    local id="$1" src="$2"
    local staged="$out_root/$id"
    [ -d "$repo_root/$src" ] || {
        echo "compile-check: source template missing: $src" >&2
        return 2
    }

    echo "== compile-check: $id =="
    rm -rf "$staged"
    mkdir -p "$staged"
    cp -r "$repo_root/$src/." "$staged/"

    # Rewrite the placeholder to the absolute repo root so the staged tree's
    # `path =` deps resolve (mirrors the staging the test used to do inline).
    grep -rlZ '@NANO_ROS_ROOT@' "$staged" 2>/dev/null | while IFS= read -r -d '' f; do
        sed -i "s#@NANO_ROS_ROOT@#$repo_root#g" "$f"
    done

    rm -f "$staged/.compile-ok"
    ( cd "$staged" && cargo check --manifest-path Cargo.toml )
    date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
    echo "   stamped $staged/.compile-ok"
}

for entry in "${COMPILE_CHECK_FIXTURES[@]}"; do
    stage_and_check "${entry%%:*}" "${entry#*:}"
done

echo "compile-check fixtures built (${#COMPILE_CHECK_FIXTURES[@]})."
