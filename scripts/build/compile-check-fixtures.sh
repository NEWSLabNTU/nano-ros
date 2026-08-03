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
# Add a `[[compile_check_fixture]]` row to `examples/fixtures.toml` (phase-319 W2).
set -Eeuo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cd "$repo_root"

# shellcheck source=scripts/build/cargo.sh
source "$repo_root/scripts/build/cargo.sh"

out_root="$repo_root/build/compile-check"
mkdir -p "$out_root"

# id : source template dir (carries @NANO_ROS_ROOT@ placeholders)

# Per-id staging hook: overwrite files in the staged tree before `cargo check`.
# Used by the n9 forms — each is the same workspace with a different
# `nros::main!(...)` invocation in the Entry pkg's main.rs.
post_stage() {
    local id="$1" staged="$2"
    local main_rs="$staged/src/demo_entry/src/main.rs"
    case "$id" in
        n9_form1)
            printf '//! n9 form 1 (no args).\n\nnros::main!();\n' > "$main_rs" ;;
        n9_form2)
            printf '//! n9 form 2 (board only).\n\nnros::main!(board = ::nros_board_linux::LinuxBoard);\n' > "$main_rs" ;;
        n9_form3)
            # phase-330 W7 — the canonical multi-node form is INPUT-addressed.
            printf '//! n9 form 3 (launch, default — the canonical multi-node form).\n\nnros::main!(launch = "demo_bringup");\n' > "$main_rs" ;;
        n9_form4)
            # phase-330 W7.g — the ONE compile proof that the DEPRECATED
            # `model =` arm still works during its window; resolves the BUILD
            # artifact (the sync below materialises it) via the ladder.
            printf '//! n9 form 4 (all explicit: board + explicit model file — DEPRECATED arm).\n\nnros::main!(\n    board = ::nros_board_linux::LinuxBoard,\n    model = "demo_bringup:config/system_model.yaml",\n);\n' > "$main_rs" ;;
        orch_tiers_single)
            # Strip the tier table so the macro takes the legacy single-tier
            # BoardEntry::run path (RFC-0032 §5 gate G.4).
            #
            # phase-319 W2 — this used to strip `[tiers.*]` from system.toml, but
            # the entry is `nros::main!(model = ...)` and phase-296 made the
            # MODEL authoritative: the strip stopped doing anything, the fixture
            # kept emitting multi-tier, and
            # `single_tier_system_takes_the_legacy_boardentry_run_path` went red.
            # Strip both — the model because it is what the macro reads, the
            # system.toml because a stale copy there would be a second source of
            # truth (issue 0351's theme one layer over).
            local model="$staged/src/demo_bringup/config/system_model.yaml"
            if [ -f "$model" ]; then
                python3 - "$model" <<'PYSTRIP'
import sys
path = sys.argv[1]
out, skip = [], False
for line in open(path):
    if line.startswith("  tiers:"):
        skip = True
        continue
    if skip:
        # The tier table ends at the next key at the same indent (e.g. bindings:).
        if line.startswith("  ") and not line.startswith("   ") and line.strip():
            skip = False
        else:
            continue
    out.append(line)
open(path, "w").writelines(out)
PYSTRIP
            fi
            local sys="$staged/src/demo_bringup/system.toml"
            if [ -f "$sys" ]; then
                sed -n '0,/^\[tiers\./{/^\[tiers\./!p}' "$sys" > "$sys.tmp" && mv "$sys.tmp" "$sys"
            fi ;;
        *) : ;;  # no overlay (orch_tiers_multi uses the fixture verbatim)
    esac
}

# Build fixtures (id : src): same staging, but `cargo build -p demo_entry`
# producing a runnable binary at build/compile-check/<id>/target/debug/demo_entry
# that the test executes (e.g. boot/run-tier assertions). The compile is still
# the build stage; the test runs the prebuilt binary.

stage_tree() {
    local id="$1" src="$2" staged="$3"
    [ -d "$repo_root/$src" ] || {
        echo "compile-check: source template missing: $src" >&2
        return 2
    }
    rm -rf "$staged"
    mkdir -p "$staged"
    cp -r "$repo_root/$src/." "$staged/"
    # Rewrite the placeholder to the absolute repo root so the staged tree's
    # `path =` deps resolve (mirrors the staging the test used to do inline).
    # NOTE the `|| true`: under `set -euo pipefail`, `find -exec grep +` exits
    # nonzero when NO staged file contains the placeholder (grep's no-match exit
    # propagates through find), which would abort the whole run for any fixture
    # that doesn't use that placeholder. The rewrite is best-effort — a missing
    # placeholder is a no-op, not an error.
    find "$staged" -type f -exec grep -lZ '@NANO_ROS_ROOT@' {} + 2>/dev/null \
        | xargs -0 -r sed -i "s#@NANO_ROS_ROOT@#$repo_root#g" || true
    post_stage "$id" "$staged"
    # phase-330 W7.g — templates no longer carry committed SystemModels
    # (W4.a); resolve them into the staged workspace's build dir the same way
    # a user build does. AFTER post_stage, so form/tier rewrites of the
    # INPUTS (main.rs, system.toml) are what gets resolved.
    # Any staged pkg (package.xml) can be a bringup — system.toml is OPTIONAL
    # to the resolver (o4's bringup is launch/ + package.xml only).
    if find "$staged" -maxdepth 3 -name package.xml -print -quit 2>/dev/null | grep -q .; then
        local _sync_cli="${NROS_CLI_BIN:-${NROS_CLI:-$(command -v nros || true)}}"
        if [ -z "$_sync_cli" ]; then
            echo "compile-check: nros CLI not found — cannot resolve staged models (just setup-cli)" >&2
            return 2
        fi
        ( cd "$staged" && "$_sync_cli" sync >/dev/null )
    fi
}

stage_and_check() {
    local id="$1" src="$2"
    local staged="$out_root/$id"
    echo "== compile-check: $id =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    ( cd "$staged" && cargo check --manifest-path Cargo.toml )
    date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
    echo "   stamped $staged/.compile-ok"
}

stage_and_build() {
    local id="$1" src="$2" manifest_dir="${3:-.}" pkg="${4:-demo_entry}"
    local staged="$out_root/$id"
    echo "== build-fixture: $id =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    # `manifest_dir` (3rd `id:src:dir` field) builds a member that lives in a
    # subdir excluded from the root workspace (e.g. O.5's `demo_entry/`, O.3's
    # `posix_entry/`). `pkg` (4th field) names the package when it isn't the
    # default `demo_entry` (O.3 builds `posix_entry`).
    ( cd "$staged" && cargo build -p "$pkg" --manifest-path "$manifest_dir/Cargo.toml" )
    date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
    echo "   built $staged/$manifest_dir/target/debug/$pkg"
}

# cmake fixtures (id : template-dir relative to repo). Configure + build a C/C++
# template into a PERSISTENT build dir (build/cmake-fixtures/<id>) so the test
# can inspect generated TUs / link sidecars / depfiles AND run/`nm` the produced
# executable — instead of running cmake at test time (issue 0034). The codegen
# step shells the `nros` CLI; the build is skipped (no stamp → test skips/fails
# per tier) when cmake or a `codegen entry`-capable `nros` is unavailable.
cmake_out="$repo_root/build/cmake-fixtures"

cmake_fixture_prereqs_ok() {
    command -v cmake >/dev/null 2>&1 || { echo "cmake-fixtures: cmake absent — skipping" >&2; return 1; }
    local nb="${NROS_CLI:-$(command -v nros || true)}"
    [ -n "$nb" ] || { echo "cmake-fixtures: nros CLI absent — skipping" >&2; return 1; }
    "$nb" codegen entry --help >/dev/null 2>&1 || {
        echo "cmake-fixtures: nros lacks 'codegen entry' — skipping" >&2; return 1; }
    # The C/mixed Entry templates parse launch XML via play_launch_parser.
    command -v play_launch_parser >/dev/null 2>&1 || {
        echo "cmake-fixtures: play_launch_parser absent — skipping (source ./activate.sh)" >&2; return 1; }
    NROS_CLI_BIN="$nb"
    return 0
}

build_cmake_fixture() {
    local id="$1" src="$2"
    local bld="$cmake_out/$id"
    [ -d "$repo_root/$src" ] || { echo "cmake-fixtures: template missing: $src" >&2; return 2; }
    echo "== cmake-fixture: $id =="
    rm -rf "$bld"
    mkdir -p "$bld"
    # Put the `nros setup`-provisioned Corrosion (source-built into
    # `~/.nros/sdk/corrosion/<ver>/`) on CMAKE_PREFIX_PATH so a fixture's
    # `find_package(Corrosion)` resolves it. Harmless for fixtures that only
    # `find_package(Corrosion QUIET)` (no Rust import); required by the
    # threadx-bringup fixture, whose codegen helper imports the Rust component
    # crates via Corrosion when present. cmake's config search walks
    # `<prefix>/<name>*/...`, so the un-versioned parent prefix suffices.
    local prefix_path="${CMAKE_PREFIX_PATH:-}"
    if [ -d "$HOME/.nros/sdk/corrosion" ]; then
        prefix_path="$HOME/.nros/sdk/corrosion${prefix_path:+:$prefix_path}"
    fi
    # Pass both nros cmake vars — different templates name it differently
    # (NROS_CLI_BIN vs NROS_BIN); the unused one is harmless.
    CMAKE_PREFIX_PATH="$prefix_path" \
        cmake -S "$repo_root/$src" -B "$bld" "-DNROS_CLI_BIN=$NROS_CLI_BIN" "-DNROS_BIN=$NROS_CLI_BIN"
    CMAKE_PREFIX_PATH="$prefix_path" cmake --build "$bld" -j
    echo "   built $bld"
}

# Cross-target build fixtures (id : src : subdir : pkg : target [: profiles]). Stage
# the template, then `cargo build --target <target> -p <pkg>` from <staged>/<subdir>
# — for firmware Entry-pkg fixtures whose codegen artifact (run_plan.rs) the test
# inspects. Gated on the rust target being installed; absent → no stamp → skip.
# The optional 6th field is a comma-separated profile list (default `debug`): a
# fixture that names `debug,release` stages ONCE and builds both profiles into the
# same tree, so a test can boot the -O0 debug ELF (fast link) OR the -O3 release
# ELF (needed when a -O0 zenoh-pico is too slow to finish a session handshake in
# budget — phase-281 W1 / the connected orch_tiers_freertos test).

stage_and_cross_build() {
    local id="$1" src="$2" subdir="$3" pkg="$4" target="$5" profiles="${6:-debug}"
    local staged="$out_root/$id"
    if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
        echo "cross-build: target $target not installed — skipping $id" >&2
        return 0
    fi
    echo "== cross-build: $id ($pkg @ $target, profiles: $profiles) =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    # firmware fixtures read the freertos platform sources + cffi headers from
    # the repo via env (the build.rs codegen + cc compile). Build every requested
    # profile into the one staged tree (debug → target/<t>/debug, release →
    # target/<t>/release).
    #
    # phase-336 note: `profiles` here is a manifest-supplied list of TARGET
    # DIRECTORY names (`debug`, `release`), not cargo profile names — `debug` is
    # the directory `dev` writes to. That is why this maps by hand instead of
    # calling `nros profile args`, which speaks profile names.
    local profile
    for profile in ${profiles//,/ }; do
        local profile_flag=()
        [ "$profile" = "release" ] && profile_flag=(--release)
        echo "   -- profile: $profile"
        ( cd "$staged/$subdir" \
            && NROS_PLATFORM_FREERTOS_SRC="$repo_root/packages/platform/nros-platform-freertos/src" \
               NROS_PLATFORM_CFFI_INCLUDE="$repo_root/packages/platform/nros-platform-api/include" \
               cargo build "${profile_flag[@]}" --target "$target" -p "$pkg" )
    done
    date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
    echo "   built $staged/$subdir (target/$target; profiles: $profiles)"
}

# phase-319 W2 (issue 0351) — the fixture INVENTORY lives in
# `examples/fixtures.toml`, not in arrays here. Six hardcoded colon-delimited
# arrays used to sit at this spot; `check-fixtures-stale.sh` enumerates the
# manifest, so it could not see any of them (issue 0350 hid there for three
# days), and AGENTS.md:79 already said they belong in the manifest.
#
# The per-builder functions below are unchanged — only where the list comes from
# moved. Record fields (\x1f-separated):
#   id, builder, dir, pkg, manifest_dir, target, profiles, output
#
# `NROS_FIXTURE_ID=<id>` narrows to one row, matching workspace-fixtures-build.sh.
id_filter="${NROS_FIXTURE_ID:-}"

compile_check_records() {
    python3 "$repo_root/scripts/build/fixtures-manifest.py" list-compile-checks \
        --builder "$1" ${id_filter:+--id "$id_filter"}
}

# Issue 0406 — a narrowing that selects no compile-check row used to run every
# per-builder loop over an empty list and finish "successfully". Decide once,
# up front, whether that emptiness is a benign cross-builder sweep miss or a
# broken invocation; the guard owns the distinction.
if [ -n "$id_filter" ]; then
    _cc_matched=0
    for _cc_builder in cargo-check cargo-build cross-build cmake-configure cxx-syntax; do
        _cc_matched=$((_cc_matched + $(compile_check_records "$_cc_builder" | wc -l)))
    done
    if [ "$_cc_matched" -eq 0 ]; then
        # shellcheck source=scripts/build/fixture-id-guard.sh
        source "$repo_root/scripts/build/fixture-id-guard.sh"
        nros_fixture_id_no_match "$id_filter" env compile_check_fixture "" ""
        exit 0
    fi
    unset _cc_matched _cc_builder
fi


# phase-319 W3 (issue 0351) — record the build INPUTS after a successful build.
#
# `.compile-ok` says only THAT a build succeeded, never what from, so a source
# edit left it valid-looking forever. `.inputsig` is the workspace lane's answer
# (`workspace-fixture-signature.sh`): written only on success, recomputed and
# compared by the staleness probe. A failed build leaves the OLD signature
# untouched — but its `.compile-ok`/artifact was already removed by the builder,
# so "failed" and "stale" both surface, never "fresh".
# phase-319 W3 (issue 0351) — mark a fixture whose build FAILED, so the test-side
# resolver can tell "broken" from "toolchain absent". Both used to present as a
# missing artifact, and the light tier skipped on both — which is how issue 0350
# stayed green while this whole lane was red.
#
# Cleared at the start of every attempt (same discipline as `.compile-ok`), so a
# marker only ever describes the most recent run.
clear_build_failed() {
    rm -f "$1/.build-failed" 2>/dev/null || true
}

mark_build_failed() {
    local stamp_dir="$1" id="$2" builder="$3"
    mkdir -p "$stamp_dir"
    printf 'fixture %s (builder %s) failed to build at %s\n' \
        "$id" "$builder" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$stamp_dir/.build-failed"
}

# Run one builder, marking + re-raising on failure. `set -e` still aborts the
# script afterwards (fail-fast is deliberate); the marker is what survives for
# the resolver.
#
# The builder runs in a SUBSHELL with its own `set -e`, NOT as `if ! builder`.
# Bash suppresses errexit for the entire body of a function invoked in a
# condition context, so `if ! build_cmake_fixture …` let a failing `cmake -S`
# fall through to the next line and the function returned its trailing `echo`'s
# status — a broken fixture reported as built. Caught by this phase's own
# acceptance test; the subshell keeps errexit live where the work happens and
# still lets us handle the status.
# Marking uses an ERR TRAP, not a status check, because bash disables errexit for
# anything in a condition context — `if ! builder`, `builder || rc=$?`, AND a
# `( set -e; builder )` subshell inside such a list all let a failing `cmake -S`
# fall through to the next line, so the function returned its trailing `echo`'s
# status and a broken fixture reported as BUILT. Both wrong shapes were caught by
# this phase's own acceptance test before landing.
#
# With the trap there is no condition context: the builder is called bare, so
# errexit still aborts the script (fail-fast is deliberate) and the trap records
# WHICH fixture died on the way out. Needs `set -E` so functions inherit it.
CURRENT_FIXTURE_STAMP_DIR=""
CURRENT_FIXTURE_ID=""
CURRENT_FIXTURE_BUILDER=""

on_fixture_err() {
    [ -n "$CURRENT_FIXTURE_STAMP_DIR" ] || return 0
    mark_build_failed "$CURRENT_FIXTURE_STAMP_DIR" "$CURRENT_FIXTURE_ID" "$CURRENT_FIXTURE_BUILDER"
}
trap on_fixture_err ERR

run_fixture() {
    local stamp_dir="$1" id="$2" builder="$3"; shift 3
    clear_build_failed "$stamp_dir"
    CURRENT_FIXTURE_STAMP_DIR="$stamp_dir"
    CURRENT_FIXTURE_ID="$id"
    CURRENT_FIXTURE_BUILDER="$builder"
    "$@"
    CURRENT_FIXTURE_STAMP_DIR=""
}

write_compile_check_sig() {
    local record="$1" stamp_dir="$2"
    mkdir -p "$stamp_dir"
    bash "$repo_root/scripts/build/compile-check-signature.sh" "$record" \
        > "$stamp_dir/.inputsig" 2>/dev/null || rm -f "$stamp_dir/.inputsig"
}

# cargo-check. A row with a TARGET is an in-place cross-check of an existing
# example dir; without one it is a staged `cargo check` whose stamp is the proof.
while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    [ -n "$target" ] && continue
    run_fixture "$out_root/$id" "$id" "$builder" stage_and_check "$id" "$dir"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(compile_check_records cargo-check)

while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    run_fixture "$out_root/$id" "$id" "$builder" \
        stage_and_build "$id" "$dir" "${mdir:-.}" "${pkg:-demo_entry}"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(compile_check_records cargo-build)

while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    run_fixture "$out_root/$id" "$id" "$builder" \
        stage_and_cross_build "$id" "$dir" "${mdir:-.}" "$pkg" "$target" "${profiles:-debug}"
    write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
done < <(compile_check_records cross-build)
# C++ syntax-only compile-checks (id : snippet.cpp under
# packages/testing/nros-tests/fixtures/cpp_compat_snippets/). `c++ -fsyntax-only`
# the snippet against the nros-cpp / nros-c / compat include set — a compile-only
# proof the public C++ API headers type-check. Stamped into build/compile-check
# (same resolver as the cargo compile-checks).
snippet_dir="$repo_root/packages/testing/nros-tests/fixtures/cpp_compat_snippets"

cxx_syntax_check() {
    local id="$1"
    local src="$snippet_dir/$id.cpp"
    local staged="$out_root/$id"
    [ -f "$src" ] || { echo "cxx-syntax: snippet missing: $src" >&2; return 2; }
    echo "== cxx-syntax: $id =="
    mkdir -p "$staged"
    rm -f "$staged/.compile-ok"
    local cxx="${CXX:-c++}"
    # Issue #34 — the per-build generated config headers MUST precede the
    # source include dir: `packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h`
    # is a stub that `#error`s, so if it is searched first the real header
    # (`target/nros-cpp-generated/nros/...`, emitted by nros-cpp's build.rs) is
    # never reached. Prepend the generated dirs.
    local inc=()
    [ -f "$repo_root/target/nros-cpp-generated/nros/nros_cpp_config_generated.h" ] \
        && inc+=(-I "$repo_root/target/nros-cpp-generated")
    [ -f "$repo_root/target/nros-c-generated/nros/nros_config_generated.h" ] \
        && inc+=(-I "$repo_root/target/nros-c-generated")
    # phase-329 W5 — the platform-header snippets `#include <nros/platform.h>`,
    # which lives ONLY in nros-platform-api (the RFC-0042 D1 canonical header).
    # Prepend it so it resolves; unique location, so no shadowing risk.
    inc+=(-I "$repo_root/packages/platform/nros-platform-api/include"
          -I "$repo_root/packages/api/nros-cpp/include"
          -I "$repo_root/packages/api/nros-c/include"
          -I "$repo_root/cmake/compat/include")
    # Best-effort: a snippet that doesn't compile (pre-existing API drift or a
    # missing generated header) does NOT fail build-test-fixtures — it just
    # leaves no `.compile-ok`, so the consuming test reports the gap per tier
    # (hard-fail full / [SKIPPED] light). The compile error is in this log.
    if "$cxx" -std=c++14 -fsyntax-only "${inc[@]}" "$src"; then
        date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
        echo "   stamped $staged/.compile-ok"
    else
        echo "   cxx-syntax FAILED for $id (no stamp; consuming test will report)" >&2
    fi
}

cmake_n=0
if cmake_fixture_prereqs_ok; then
    mkdir -p "$cmake_out"
    while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
        [ -n "$id" ] || continue
        run_fixture "$cmake_out/$id" "$id" "$builder" build_cmake_fixture "$id" "$dir"
        write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$cmake_out/$id"
        cmake_n=$((cmake_n + 1))
    done < <(compile_check_records cmake-configure)
    # Phase 246 — the ThreadX `threadx_bringup_rv64` configure-only baker-audit
    # leg is retired with `NanoRosThreadxSystemCodegen.cmake`; the bare-metal
    # riscv64 typed-carrier examples (examples/qemu-riscv64-threadx/{c,cpp}/*)
    # cover the real path.
fi

cxx_n=0
if command -v "${CXX:-c++}" >/dev/null 2>&1; then
    # Issue #34 — generate the per-build config headers the snippets need
    # (`nros_cpp_config_generated.h` / `nros_config_generated.h`). nros-cpp's /
    # nros-c's build.rs emit them under `target/nros-{cpp,c}-generated/` on a host
    # build; `cxx_syntax_check` then prepends those dirs. Best-effort: if the host
    # cargo build fails, the headers stay absent and the snippets that include
    # them leave no stamp (consuming test reports the gap per tier). The sizes
    # need not be exact — this is a `-fsyntax-only` check, not a link.
    echo "== generating nros-cpp / nros-c config headers for cxx-syntax =="
    ( cd "$repo_root" && cargo build -q -p nros-cpp -p nros-c --features nros-cpp/ros-humble ) \
        || echo "cxx-syntax: config-header generation build failed (snippets needing them will skip)" >&2
    while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
        [ -n "$id" ] || continue
        run_fixture "$out_root/$id" "$id" "$builder" cxx_syntax_check "$id"
        write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
        cxx_n=$((cxx_n + 1))
    done < <(compile_check_records cxx-syntax)
else
    echo "cxx-syntax: no C++ compiler — skipping" >&2
fi

# cargo-check of an existing example dir for a cross target (id : dir : target).
# Proves an example's `nros::main!()` emit type-checks WITHOUT linking — for
# examples that intentionally don't link standalone (e.g. talker-embassy lacks
# the board memory layout). Stamped into build/compile-check (same resolver).
# Gated on the rust target being installed; absent → no stamp → test skips.
cargo_check_n=0
while IFS=$'\x1f' read -r id builder dir pkg mdir target profiles output; do
    [ -n "$id" ] || continue
    # Only the target-bearing cargo-check rows reach here; the staged ones ran above.
    [ -n "$target" ] || continue
    [ -d "$repo_root/$dir" ] || { echo "cargo-check: example missing: $dir" >&2; continue; }
    if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
        echo "cargo-check: target $target not installed — skipping $id" >&2
        continue
    fi
    echo "== cargo-check: $id ($target) =="
    mkdir -p "$out_root/$id"
    rm -f "$out_root/$id/.compile-ok"
    if ( cd "$repo_root/$dir" && cargo check --target "$target" ); then
        date -u +%Y-%m-%dT%H:%M:%SZ > "$out_root/$id/.compile-ok"
        echo "   stamped $out_root/$id/.compile-ok"
        write_compile_check_sig "$id$(printf '\x1f')$builder$(printf '\x1f')$dir$(printf '\x1f')$pkg$(printf '\x1f')$mdir$(printf '\x1f')$target$(printf '\x1f')$profiles$(printf '\x1f')$output" "$out_root/$id"
        cargo_check_n=$((cargo_check_n + 1))
    else
        echo "   cargo-check FAILED for $id (no stamp)" >&2
    fi
done < <(compile_check_records cargo-check)

# px4 xrce companion examples (#102 / #136 debt). Compile-check only: the
# runtime needs PX4 SITL + a Micro-XRCE-DDS agent, but the generated CDR
# bindings must at least type-check. `px4_msgs` is generated from the vendored
# PX4-Autopilot `.msg` tree by `nros generate-px4-msgs` (no ament/pip dep, just
# the submodule) into each example's gitignored `generated/`. Gated on that
# submodule being checked out; absent → no stamp → the coverage gate keeps
# these as tracked leaves rather than a silent gap.
PX4_XRCE_EXAMPLES=(
    "px4_probe:examples/px4/rust/companion/px4-probe"
    "px4_stub:examples/px4/rust/companion/px4-stub"
    "px4_offboard_companion:examples/px4/rust/companion/offboard-companion"
)
px4_autopilot_dir="$repo_root/third-party/px4/PX4-Autopilot"
px4_n=0
if [ -d "$px4_autopilot_dir/msg" ] && command -v nros >/dev/null 2>&1; then
    for entry in "${PX4_XRCE_EXAMPLES[@]}"; do
        id="${entry%%:*}"; dir="${entry#*:}"
        [ -d "$repo_root/$dir" ] || { echo "px4: example missing: $dir" >&2; continue; }
        echo "== px4-compile-check: $id =="
        if ! nros generate-px4-msgs --px4 "$px4_autopilot_dir" \
                --output "$repo_root/$dir/generated"; then
            echo "   px4_msgs codegen FAILED for $id (no stamp)" >&2
            continue
        fi
        mkdir -p "$out_root/$id"
        rm -f "$out_root/$id/.compile-ok"
        if ( cd "$repo_root/$dir" && cargo check ); then
            date -u +%Y-%m-%dT%H:%M:%SZ > "$out_root/$id/.compile-ok"
            echo "   stamped $out_root/$id/.compile-ok"
            px4_n=$((px4_n + 1))
        else
            echo "   cargo-check FAILED for $id (no stamp)" >&2
        fi
    done
else
    echo "px4: PX4-Autopilot submodule absent (third-party/px4/PX4-Autopilot) — skipping px4 compile-check" >&2
fi

# phase-319 W2 — counts come from the manifest now, not from array lengths.
check_n="$(compile_check_records cargo-check | wc -l)"
build_n="$(compile_check_records cargo-build | wc -l)"
echo "fixtures built (check=$check_n build=$build_n cmake=$cmake_n cxx=$cxx_n cargo-check=$cargo_check_n px4=$px4_n)."
