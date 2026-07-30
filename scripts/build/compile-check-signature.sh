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
    nros_bin="$repo_root/packages/cli/target/release/nros"
    if [ -x "$nros_bin" ]; then
        bin_hash="$(sha256sum "$nros_bin" | awk '{print $1}')"
        cache="$repo_root/.nros-cache/codegen-fingerprint/$bin_hash"
        if [ -r "$cache" ]; then
            fp="$(cat "$cache")"
        elif fp="$("$nros_bin" codegen-fingerprint 2>/dev/null)" && [ -n "$fp" ]; then
            mkdir -p "$(dirname "$cache")" && printf '%s' "$fp" > "$cache" || true
        else
            fp="$bin_hash"
        fi
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
            "packages/core/nros-cpp/include"
            "packages/core/nros-c/include"
        )
    else
        sig_paths=("$dir")
    fi

    # Enumerate via the git index, not a filesystem walk (phase-300 W2.1): build
    # byproducts under gitignored trees can never leak into the signature, which
    # is what made the old find-based walk both slow and falsely stale.
    git -C "$repo_root" ls-files -z --cached --others --exclude-standard -- "${sig_paths[@]}" 2>/dev/null \
        | sort -z \
        | while IFS= read -r -d '' rel; do
            case "$rel" in
                *.c|*.cc|*.cpp|*.h|*.hpp|*.rs|*.toml|*.xml|*.yaml|*.lock|*/CMakeLists.txt|*/package.xml|*.cmake)
                    printf 'path:%s\0' "$rel"
                    cat "$repo_root/$rel"
                    printf '\0'
                    ;;
            esac
        done
} | sha256sum | awk '{print $1}'
