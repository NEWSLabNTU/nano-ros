#!/usr/bin/env bash
# One example leaf's lint: rustfmt --check, then clippy. `just native check`
# runs this per leaf, from BOTH its serial and its pooled branch, so the two
# cannot drift (they had: three copies of the per-leaf derivation).
#
# Usage (repo root): lint-example-leaf.sh <leaf-dir>
# Env (exported by the recipe): NIGHTLY, NROS_EXAMPLE_LINT_DIR,
#   NROS_EXAMPLE_PROFILE_FLAGS, SSID/PASSWORD (esp32).
#
# Issue 1882: a board-image leaf (every non-`native` platform with a
# `system.toml`) is not a standalone cargo project. Its manifest names the
# `<leaf>_nros_selection` crate and the in-repo `[patch.crates-io]` rows, and
# cargo gets its target, link set and build-std from
# `build/<image>/nros-cargo.toml` -- all RENDERED by `nros sync` (RFC-0098 D1,
# phase-445 W6), none tracked. Linted bare, 27 leaves failed at manifest parse
# (`no matching package named nros`, `failed to load manifest for dependency
# ..._nros_selection`) -- the red six weeks of no lane hid. So render first, and
# hand cargo the rendered config, exactly as `nros build` does.
set -euo pipefail
dir="${1:?leaf dir}"
platform="$(echo "$dir" | cut -d/ -f2)"
flags=""
env_prefix=()
if [ "$platform" != "native" ]; then flags="${NROS_EXAMPLE_PROFILE_FLAGS:-}"; fi
case "$platform" in
    esp32 | esp32-c3-baremetal) env_prefix=(env "SSID=${SSID:-test}" "PASSWORD=${PASSWORD:-test}") ;;
esac
if [ -n "${NROS_CHECK_VERBOSE:-}" ]; then echo "  check $dir"; fi

# `-p <leaf>`, not a bare `fmt --check`: once a developer has run `nros sync`
# the leaf's workspace includes the `generated/*` message crates, user-side
# codegen output that rustfmt has no business judging.
pkg="$(sed -n 's/^name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$dir/Cargo.toml" | head -1)"
# One target dir PER LEAF (each is its own workspace root, RFC-0026; issue 0616),
# never the leaf's own `target/` (issue 0624).
lintdir="$NROS_EXAMPLE_LINT_DIR/$(echo "${dir#examples/}" | tr '/' '-')"

cfg=()
if [ "$platform" != "native" ] && [ -f "$dir/system.toml" ]; then
    # `nros sync` is idempotent; its chatter is not this gate's output, so it is
    # kept for the failure path only.
    log="$(mktemp "${TMPDIR:-/tmp}/nros_lint_sync.XXXXXX")"
    if ! (cd "$dir" && nros sync) >"$log" 2>&1; then
        echo "native check: \`nros sync\` failed in $dir" >&2
        cat "$log" >&2
        rm -f "$log"
        exit 1
    fi
    rm -f "$log"
    # `build/nros*` are sync's own dirs; every other `build/<id>/` is an image.
    for f in "$dir"/build/*/nros-cargo.toml; do
        [ -f "$f" ] && cfg+=(--config "${f#"$dir"/}")
    done
fi

cd "$dir"
cargo "+$NIGHTLY" fmt --check -p "$pkg"
# shellcheck disable=SC2086  # $flags is a word list by design
"${env_prefix[@]}" cargo clippy --quiet "${cfg[@]}" --target-dir "$lintdir" $flags
