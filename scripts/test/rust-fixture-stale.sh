#!/usr/bin/env bash
# Print a rust fixture's id if cargo considers it stale — reusing cargo's own
# fingerprint instead of a custom input hash (Phase 177.9 / 181).
#
# Input: ONE manifest record line (from `fixtures-manifest.py list`), TAB-
# separated: <dir>\t<env>\t<cargo-args>. Building with the fixture's EXACT
# features/target-dir/env (not default features) is required — mismatched
# features make cargo rebuild on every probe (feature thrash) and report false
# staleness.
#
# `cargo build --message-format=json` is a no-op when fresh and rebuilds only
# stale units; a `"fresh":false` artifact means the fixture was stale (and is
# now rebuilt). Must be invoked from the repo root.
set -u

line="$1"
IFS=$'\t' read -r dir envstr cargo_args <<< "$line"
[ -n "${dir:-}" ] || exit 0

# shellcheck source=/dev/null
source scripts/build/cargo.sh 2>/dev/null || exit 0
prof_args="$(nros_cargo_profile_arg_string)"

# `env $envstr` is a no-op when envstr is empty; $cargo_args / $prof_args are
# intentionally word-split into cargo flags.
# shellcheck disable=SC2086
if ( cd "$dir" && env $envstr cargo build $prof_args $cargo_args \
        --message-format=json --quiet 2>/dev/null ) | grep -q '"fresh":false'; then
    echo "$dir${cargo_args:+ ($cargo_args)}"
fi
