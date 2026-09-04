#!/usr/bin/env bash
# phase-319 W3 (issue 0351) — build-input signature for ONE compile-check fixture.
#
# The lane's `.compile-ok` stamps record only THAT a build succeeded, never what
# from, so a source edit left them valid-looking forever and a failed build was
# indistinguishable from one that never ran. This is the workspace lane's answer
# (`workspace-fixture-signature.sh`) applied to the compile-check lane: hash the
# manifest record plus the row's source tree, write it after a SUCCESSFUL build,
# and let the staleness probe recompute and compare.
#
# Usage: compile-check-signature.sh <manifest-record>
#   record fields (\x1f): id, builder, dir, pkg, manifest_dir, target, profiles, output
set -euo pipefail
record="${1:?usage: compile-check-signature.sh <manifest-record>}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

NROS_REPO_ROOT="$repo_root"
# shellcheck source=scripts/build/build-root.sh
source "$script_dir/build-root.sh"
# shellcheck source=scripts/build/source-manifest.sh
source "$script_dir/source-manifest.sh"
# shellcheck source=scripts/build/codegen-fingerprint.sh
source "$script_dir/codegen-fingerprint.sh"

IFS=$'\x1f' read -r id builder dir _pkg _mdir _target _profiles _output <<< "$record"
[ -n "$id" ] && [ -n "$builder" ] || {
    echo "compile-check record is missing id/builder" >&2
    exit 2
}

{
    printf 'nros-compile-check-fixture-v1\0%s\0' "$record"

    # The staged tree is emitted by the nros CLI for rows whose build runs
    # codegen (cmake-configure drives `nros codegen-system`; the cargo rows
    # expand `nros::main!`, which reads models the CLI produced). A signature
    # blind to the tool repeats issue #182 — a museum emitter verifying as fresh.
    # Same fingerprint ladder phase-318 W1 established for the workspace lane:
    # prefer what the tool EMITS (stable across unrelated rebuilds), fall back to
    # the binary hash, never to "assume fresh".
    # The ladder itself lives in `codegen-fingerprint.sh` (issue 1018) — this
    # lane and the workspace lane had a copy each, already drifted on the cache
    # test (`-r` here, `-s` there) and on the fallback spelling. Same signature
    # bytes: this lane's fallback carries no prefix, so it passes none.
    if fp="$(nros_codegen_fingerprint "$repo_root")"; then
        printf 'tool:nros\0%s\0' "$fp"
    else
        printf 'tool:nros-absent\0'
    fi

    # `cxx-syntax` rows carry no dir — the snippet is resolved by id under
    # fixtures/cpp_compat_snippets/, and the headers it checks are the real
    # input. Hash the snippet plus the public C/C++ include trees.
    if [ "$builder" = "cxx-syntax" ]; then
        sig_paths=(
            "packages/testing/nros-tests/fixtures/cpp_compat_snippets/$id.cpp"
            "packages/api/nros-cpp/include"
            "packages/api/nros-c/include"
        )
    else
        sig_paths=("$dir")
    fi

    # phase-360 W3 — ONE spelling, in `source-manifest.sh`, shared with the
    # workspace lane. It enumerates through the git index (so gitignored build
    # trees can never leak in, which is what made the old find-based walk both
    # slow and falsely stale) and hashes EVERY file it finds — no extension
    # allowlist. The allowlist that used to live here dropped 8 `.conf` and 3
    # `.msg` under these rows, both real build inputs, and had drifted apart
    # from the workspace lane's copy.
    nros_source_manifest "$repo_root" "${sig_paths[@]}"

    # phase-360 W4 — plus the closure the build MEASURED. `sig_paths` above is
    # the row's own dir, and a compile-check row exists to compile AGAINST
    # workspace crates that are not in it; issue 0466 records the gate staying
    # silent while `nros-board-common/src/platform_config.rs` changed. Cargo
    # already wrote that answer as dep-info, so read it instead of guessing.
    # Was "empty (and therefore inert) for rows with no cargo dep-info —
    # cxx-syntax, cmake-configure, west-*". No longer true: the 2026-08-15
    # extension gave every builder a measured record — `cxx-syntax` compiles
    # with `-MD -MF` (which composes with `-fsyntax-only`: no object, still a
    # dep list), `cmake-configure` reads `CMAKE_MAKEFILE_DEPENDS`, and `west-*`
    # reads `build.ninja`'s RERUN_CMAKE edge. The comment outlived the code and
    # would have told the next reader that these rows still guess.
    if [ "$builder" = "cmake-configure" ]; then
        _sig_build_dir="$(nros_build_dir "$NROS_KIND_CMAKE_FIXTURES" "$id")"
    else
        _sig_build_dir="$(nros_build_dir "$NROS_KIND_COMPILE_CHECK" "$id")"
    fi
    printf 'closure\0'
    nros_dep_closure_manifest "$repo_root" "$_sig_build_dir"
} | sha256sum | awk '{print $1}'
