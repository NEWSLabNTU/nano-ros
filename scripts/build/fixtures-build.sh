#!/usr/bin/env bash
# Build all <platform> [<lang>] fixtures from the SSOT manifest
# (examples/fixtures.toml). Phase 181.
#
# Per-fixture options (features / --no-default-features / --target-dir / cross
# --target / build env) come from the manifest; per-PLATFORM env (toolchain
# paths, SDK dirs, +nightly, cross target via the example's .cargo/config) is
# the caller's responsibility and must already be exported. Codegen
# (`nros generate-rust`) is also a caller/recipe concern — run it before this.
#
# Usage (from repo root):
#   scripts/build/fixtures-build.sh <platform> [<lang>]   # lang default: rust
#
# Honors NROS_JOBSERVER=1 (serial; tools inherit fifo tokens) and falls back to
# serial when GNU parallel is absent.
set -euo pipefail

platform="${1:?usage: fixtures-build.sh <platform> [lang]}"
lang="${2:-rust}"

# shellcheck source=/dev/null
source scripts/build/cargo.sh
cargo_profile_args="$(nros_cargo_profile_arg_string)"
export cargo_profile_args

nros_fixture_build_one() {
    local dir envstr args
    IFS=$'\x1f' read -r dir envstr args <<< "$1"
    [ -n "$dir" ] || return 0
    echo "  → $dir ${args}"
    # shellcheck disable=SC2086
    ( cd "$dir"; [ -n "$envstr" ] && export $envstr; cargo build $cargo_profile_args $args --quiet )
}
export -f nros_fixture_build_one

manifest() { python3 scripts/build/fixtures-manifest.py list --platform "$platform" --lang "$lang"; }

if [ "${NROS_JOBSERVER:-}" = "1" ] || ! command -v parallel >/dev/null 2>&1; then
    while IFS= read -r line; do nros_fixture_build_one "$line"; done < <(manifest)
else
    manifest | parallel --halt now,fail=1 --line-buffer -j "$(nros_cargo_frontend_jobs)" nros_fixture_build_one {}
fi
