#!/usr/bin/env bash
#
# phase-336 — no build site may NAME a cargo profile.
#
# The profile a build uses is a decision the user makes once (CMAKE_BUILD_TYPE
# for C/C++, NROS_CARGO_PROFILE for Rust) and nano-ros propagates. A literal
# `--release` in a recipe, or a `target/<triple>/release/` path in a resolver,
# opts that one site out of the decision — and the failure is quiet: the build
# writes one directory and the reader looks in another, so a fresh, working
# image reads as "fixture missing" (issue #156; the same shape again in the
# freertos/threadx run lanes and in `just native size`, whose
# `|| echo "build failed"` reported a wrong path as a broken build).
#
# THE RULE
#
#   * A cargo profile flag (`--release`, `--profile <name>`) comes from
#     `nros profile` / `scripts/build/cargo.sh` / `nros_cargo_profile::…`.
#   * A `target/…/<profile>/` path segment is derived the same way.
#
# THE OPT-OUT
#
# Mark the line — or the line above it — with:
#
#     # profile-literal-ok: <reason>
#
# The marker is local and self-documenting on purpose: a path allow-list in
# this file goes stale silently when the site moves, and the reason lives where
# the next reader is already looking. Legitimate reasons so far:
#
#   host tool       the CLI / launch-resolver builds — following the knob needs
#                   `nros profile`, which needs the CLI to exist first. The two
#                   in-tree host-binary PATHS (`packages/cli/target/release/nros`,
#                   `nros-launch-resolve/target/release`) are excluded
#                   structurally: they appear in ~10 diagnostics and locators,
#                   and they are a fixed idiom, not a build decision.
#   vendored        a third-party workspace that defines no nros-* profile
#   benchmark       wants the performance profile BY NAME, not the ambient one
#   symbol fixture  a path two tests assert on; optimization is irrelevant
#   unprofiled      built with a plain `cargo build`, so `debug/` IS derived
#   dir vocabulary  a manifest field naming target DIRECTORIES, not profiles
#
# A platform that cannot use the ambient profile is NOT an opt-out: it gets a
# carve-out in `nros-cargo-profile` (nuttx-rust, freertos-qemu) so the builder
# and every resolver read one constant.
#
# `nros-cargo-profile` itself is excluded: it is the one place that MAY name a
# profile, since defining the mapping is what it is for.
#
# Buildless and source-free, so it belongs in `check-fast`.

set -uo pipefail
cd "$(dirname "$0")/.."

MARKER='profile-literal-ok'

# A hit is exempt when its own line carries the marker, or when one of the few
# lines above it does. The window is 3 rather than 1 because a marker cannot
# always sit on the preceding line: inside a `\`-continued shell command a
# comment breaks the continuation, so the marker has to go above the whole
# recipe (just refuses to parse it otherwise).
exempt() {
    local file="$1" line="$2" text="$3"
    [[ "$text" == *"$MARKER"* ]] && return 0
    local n prev
    for n in 1 2 3; do
        [ "$((line - n))" -ge 1 ] || break
        prev="$(sed -n "$((line - n))p" "$file")"
        [[ "$prev" == *"$MARKER"* ]] && return 0
    done
    return 1
}

fail=0
scan() {
    local what="$1"
    local hit file line text
    while IFS= read -r hit; do
        file="${hit%%:*}"
        line="${hit#*:}"
        line="${line%%:*}"
        text="${hit#*:*:}"
        # A comment describing the rule is not a violation of it.
        [[ "$text" =~ ^[[:space:]]*(#|//|///) ]] && continue
        if exempt "$file" "$line" "$text"; then
            continue
        fi
        echo "[FAIL] $what: $file:$line:$text" >&2
        fail=1
    done
}

# 1. Cargo profile flags. `rustup --profile minimal` is a different tool's flag,
#    and a line that already asks the table is the fix, not the problem.
# Rust build scripts are in scope too: `nros-sizes-build` spawned a nested
# `cargo build --release` from a `PROFILE == "release"` comparison, so a
# custom-profile outer build ran a whole extra release compile at a DIFFERENT
# optimization level than the crate it was measuring (phase-336 W7).
scan "hardcoded cargo profile flag" < <(
    git grep -nE -- '(--release|--profile[= ]+[a-z][a-z0-9-]*)' \
        -- justfile 'just/*.just' 'scripts/build/*.sh' 'scripts/bootstrap.sh' \
           'packages/tooling/*/src/**' 'packages/testing/nros-tests/src/**' \
    | grep -v 'rustup' \
    | grep -vE 'nros_cargo_profile|nros profile|profile_arg|profile_args|PROFILE_FLAGS|_profile"' \
    | grep -v '^packages/tooling/nros-cargo-profile/' \
    || true)

# 2. Profile-named directory segments in artifact paths.
scan "hardcoded profile directory" < <(
    git grep -nE 'target[^ "]*/(release|debug)/' \
        -- justfile 'just/*.just' 'scripts/build/*.sh' 'scripts/test/*.sh' \
           'cmake/**' 'zephyr/cmake/**' 'packages/testing/nros-tests/src/**' \
           'packages/tooling/*/src/**' \
    | grep -vE 'packages/cli/target/release/nros|nros-launch-resolve/target/release' \
    | grep -v '^packages/tooling/nros-cargo-profile/' \
    || true)

if [ "$fail" -ne 0 ]; then
    cat >&2 <<'EOF'

A build site names a cargo profile instead of asking for the active one.

  shell:  source scripts/build/cargo.sh
          cargo build $(nros_cargo_profile_arg_string)
          …/"$(nros_cargo_target_profile_dir)"/…
  cmake:  nros_resolve_cargo_profile()   # ${NROS_CARGO_PROFILE}, ${NROS_CARGO_PROFILE_DIR}
  rust:   nros_cargo_profile::{build_args, target_dir}

If the site is genuinely outside the propagation graph, mark it:

  # profile-literal-ok: <one of host tool | vendored | benchmark |
  #                      symbol fixture | unprofiled | dir vocabulary>
EOF
    exit 1
fi

echo "build-profile literals OK — every profile flag and artifact path is derived or marked."
