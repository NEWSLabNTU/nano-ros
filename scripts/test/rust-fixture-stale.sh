#!/usr/bin/env bash
# Print the given rust fixture dir if cargo considers it stale — reusing
# cargo's own fingerprint instead of a custom input hash (Phase 177.9).
#
# `cargo build --message-format=json` is a no-op when everything is fresh and
# rebuilds (incrementally) only stale units; a `"fresh":false` artifact means
# the fixture binary was stale (and is now rebuilt). Must be invoked from the
# repo root (so `scripts/build/cargo.sh` resolves) with one example dir arg.
set -u

dir="$1"
# shellcheck source=/dev/null
source scripts/build/cargo.sh 2>/dev/null || exit 0
prof_args="$(nros_cargo_profile_arg_string)"

# Default features only (matches `just <plat> build-examples`, which builds
# most rust fixtures with a bare `cargo build`); feature/target-dir variants
# still surface source edits via their default build.
if ( cd "$dir" && cargo build $prof_args --message-format=json --quiet 2>/dev/null ) \
    | grep -q '"fresh":false'; then
    echo "$dir"
fi
