#!/usr/bin/env bash
# One example leaf's lint: rustfmt --check, then clippy. `just linux check`
# runs this per leaf, from BOTH its serial and its pooled branch, so the two
# cannot drift (they had: three copies of the per-leaf derivation).
#
# Usage (repo root): lint-example-leaf.sh <leaf-dir>
# Env (exported by the recipe): NIGHTLY, NROS_EXAMPLE_LINT_DIR,
#   NROS_EXAMPLE_PROFILE_FLAGS, SSID/PASSWORD (esp32).
#
# Issue 1882: a leaf with a `system.toml` is not a standalone cargo project.
# Its manifest names the `<leaf>_nros_selection` crate and the in-repo
# `[patch.crates-io]` rows, and cargo gets its target, link set and build-std from
# `build/<image>/nros-cargo.toml` -- all RENDERED by `nros sync` (RFC-0098 D1,
# phase-445 W6), none tracked. Linted bare, 27 leaves failed at manifest parse
# (`no matching package named nros`, `failed to load manifest for dependency
# ..._nros_selection`) -- the red six weeks of no lane hid. So render first, and
# hand cargo the rendered config, exactly as `nros build` does.
#
# Issue 1888: a `native` leaf with a `system.toml` is the same kind of project.
# Its manifest names `<leaf>_nros_selection` too, rendered into
# `build/nros/selection/` by the same sync. The first version synced only
# non-native leaves, so 20 of the 22 `examples/linux/rust/*` leaves linted only
# where an earlier fixture build had happened to sync them, and failed
# `cargo metadata` everywhere else.
#
# Issue 1892: every decision below is read off the leaf's SHAPE (the files it
# holds, `scripts/build/leaf-settings.sh`), never off its platform directory.
# 1882's `platform != "native"` test had become false (1305 put the host leaves
# on `[image.<id>]` too), and a directory test also moves when the directory is
# renamed (phase-500 W4). `check-host-platform-vocabulary` item 9 runs this
# script in PLAN mode (`NROS_LINT_LEAF_PLAN=1`: print the decisions, run
# neither sync nor cargo) over the same leaf under several platform dirs and
# fails if any decision differs. The one platform case left is esp32's
# SSID/PASSWORD env, a property of that firmware's source (dormant, issue 1525).
set -euo pipefail
dir="${1:?leaf dir}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/build/leaf-settings.sh
source "$here/leaf-settings.sh"
platform="$(echo "$dir" | cut -d/ -f2)"
# The profile the fixture lane builds every one of these leaves at. It used to
# be withheld from `native` leaves (a `--release`-for-firmware rule from before
# the named profiles); the profiles are defined in the repo-root
# `.cargo/config.toml`, which cargo discovers from every in-tree leaf.
flags="${NROS_EXAMPLE_PROFILE_FLAGS:-}"
env_prefix=()
case "$platform" in
    esp32 | esp32-c3-baremetal) env_prefix=(env "SSID=${SSID:-test}" "PASSWORD=${PASSWORD:-test}") ;;
esac
plan="${NROS_LINT_LEAF_PLAN:-}"
if [ -n "${NROS_CHECK_VERBOSE:-}" ] && [ -z "$plan" ]; then echo "  check $dir"; fi

# `-p <leaf>`, not a bare `fmt --check`: once a developer has run `nros sync`
# the leaf's workspace includes the `generated/*` message crates, user-side
# codegen output that rustfmt has no business judging.
pkg="$(sed -n 's/^name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$dir/Cargo.toml" | head -1)"

# Issue 1887: EVERY leaf `nros sync` can read is rendered, whatever its
# platform. Since phase-445 W6 no example tracks a `.cargo/` (gate
# `check-example-cargo-dirs`), so a leaf with `[image.<id>]` names its
# generated `<leaf>_nros_selection` crate, and even a leaf with no `system.toml`
# resolves `nros-log` only through the `[patch.crates-io]` row sync writes
# (`examples/linux/rust/logging`).
render=0
if nros_leaf_sync_renders "$dir"; then render=1; fi
if [ "$render" = 1 ] && [ -z "$plan" ]; then
    # `nros sync` is idempotent; its chatter is not this gate's output, so it is
    # kept for the failure path only.
    log="$(mktemp "${TMPDIR:-/tmp}/nros_lint_sync.XXXXXX")"
    if ! (cd "$dir" && nros sync) >"$log" 2>&1; then
        echo "linux check: \`nros sync\` failed in $dir" >&2
        cat "$log" >&2
        rm -f "$log"
        exit 1
    fi
    rm -f "$log"
fi
# The generated settings reach a cargo run inside the leaf through sync's
# `include` (issue 1381); `--config` only where that include is absent.
cfg=()
while IFS= read -r a; do [ -n "$a" ] && cfg+=("$a"); done < <(nros_leaf_inside_config_args "$dir")

if [ -n "$plan" ]; then
    printf 'render=%s\nconfig=%s\nflags=%s\npkg=%s\n' "$render" "${cfg[*]:-}" "$flags" "$pkg"
    exit 0
fi

# One target dir PER LEAF (each is its own workspace root, RFC-0026; issue 0616),
# never the leaf's own `target/` (issue 0624).
lintdir="$NROS_EXAMPLE_LINT_DIR/$(echo "${dir#examples/}" | tr '/' '-')"

# shellcheck source=scripts/build/gate-prune.sh
source "$(dirname "${BASH_SOURCE[0]}")/gate-prune.sh"

cd "$dir"
cargo "+$NIGHTLY" fmt --check -p "$pkg"
# Issue 2100 — on a lane that asks, the leaf's target dir goes as soon as its
# verdict is in, pass or fail: 49 of them held 36 G until the gate ended. The
# verdict is clippy's output, which is already in the log.
rc=0
# shellcheck disable=SC2086  # $flags is a word list by design
"${env_prefix[@]}" cargo clippy --quiet "${cfg[@]}" --target-dir "$lintdir" $flags || rc=$?
nros_gate_prune "$lintdir"
exit "$rc"
