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
    # n9 `nros::main!()` positive forms — all stage the same n9_workspace
    # template; `post_stage` writes the form-specific demo_entry/src/main.rs.
    "n9_form1:packages/testing/nros-tests/fixtures/n9_workspace"
    "n9_form2:packages/testing/nros-tests/fixtures/n9_workspace"
    "n9_form3:packages/testing/nros-tests/fixtures/n9_workspace"
    "n9_form4:packages/testing/nros-tests/fixtures/n9_workspace"
)

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
            printf '//! n9 form 2 (board only).\n\nnros::main!(board = ::nros_board_native::NativeBoard);\n' > "$main_rs" ;;
        n9_form3)
            printf '//! n9 form 3 (launch, default file).\n\nnros::main!(launch = "demo_bringup");\n' > "$main_rs" ;;
        n9_form4)
            printf '//! n9 form 4 (all explicit).\n\nnros::main!(\n    board = ::nros_board_native::NativeBoard,\n    launch = "demo_bringup:sim.launch.xml",\n    args = [("use_sim", "true")],\n);\n' > "$main_rs" ;;
        orch_tiers_single)
            # Strip the `[tiers.*]` blocks from system.toml so the macro takes
            # the legacy single-tier BoardEntry::run path (RFC-0032 §5 gate G.4).
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
BUILD_FIXTURES=(
    "orch_tiers_multi:packages/testing/nros-tests/fixtures/orchestration_tiers_native"
    "orch_tiers_single:packages/testing/nros-tests/fixtures/orchestration_tiers_native"
)

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
    find "$staged" -type f -exec grep -lZ '@NANO_ROS_ROOT@' {} + 2>/dev/null \
        | xargs -0 -r sed -i "s#@NANO_ROS_ROOT@#$repo_root#g"
    post_stage "$id" "$staged"
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
    local id="$1" src="$2"
    local staged="$out_root/$id"
    echo "== build-fixture: $id =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    ( cd "$staged" && cargo build -p demo_entry --manifest-path Cargo.toml )
    date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
    echo "   built $staged/target/debug/demo_entry"
}

# cmake fixtures (id : template-dir relative to repo). Configure + build a C/C++
# template into a PERSISTENT build dir (build/cmake-fixtures/<id>) so the test
# can inspect generated TUs / link sidecars / depfiles AND run/`nm` the produced
# executable — instead of running cmake at test time (issue 0034). The codegen
# step shells the `nros` CLI; the build is skipped (no stamp → test skips/fails
# per tier) when cmake or a `codegen entry`-capable `nros` is unavailable.
CMAKE_FIXTURES=(
    "cpp_robot_entry:examples/templates/multi-node-workspace-cpp"
    # Phase 240.2b — the TYPED multi-node entry: `nano_ros_entry(... TYPED)` shells
    # `nros codegen entry --typed --metadata`, the generated TU constructs each
    # component + calls `configure(node)` + `NativeBoard::run_components` (no
    # register-symbol, no interpreter). The test inspects that shape.
    "cpp_robot_entry_typed:examples/templates/multi-node-workspace-cpp-typed"
    "c_mixed_workspace:examples/templates/c-and-cpp-mixed-workspace"
    "pure_c_workspace:examples/templates/pure-c-workspace"
    # workspace-over-AMENT shadowing: the build links the workspace `std_msgs`
    # shadow (carrying Marker.msg) over the AMENT one; the test `nm`s the
    # consumer to prove which won. Needs an AMENT std_msgs in the build env.
    "shadowing:examples/templates/workspace-shadowing"
    # add_subdirectory(nano-ros) link smoke (a user project linking
    # NanoRos::NanoRos via add_subdirectory).
    "cmake_add_subdir:packages/testing/nros-tests/fixtures/cmake_add_subdirectory_smoke"
    # cpp workspace cmake configure emits nros-metadata.json (the §212.L cmake fns
    # component/application/deploy metadata) — the test inspects it.
    "metadata_cpp:examples/workspaces/cpp"
    # §212.L.9 cmake-fn metadata: each configures a tiny CMakeLists exercising
    # nano_ros_node_register / nano_ros_deploy → nros-metadata.json (test inspects).
    "l9_register_cpp:packages/testing/nros-tests/fixtures/l9_register_cpp"
    "l9_register_c:packages/testing/nros-tests/fixtures/l9_register_c"
    "l9_deploy:packages/testing/nros-tests/fixtures/l9_deploy"
)
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
    # Pass both nros cmake vars — different templates name it differently
    # (NROS_CLI_BIN vs NROS_BIN); the unused one is harmless.
    cmake -S "$repo_root/$src" -B "$bld" "-DNROS_CLI_BIN=$NROS_CLI_BIN" "-DNROS_BIN=$NROS_CLI_BIN"
    cmake --build "$bld" -j
    echo "   built $bld"
}

# Cross-target build fixtures (id : src : subdir : pkg : target). Stage the
# template, then `cargo build --target <target> -p <pkg>` from <staged>/<subdir>
# — for firmware Entry-pkg fixtures whose codegen artifact (run_plan.rs) the test
# inspects. Gated on the rust target being installed; absent → no stamp → skip.
CROSS_BUILD_FIXTURES=(
    "freertos_firmware:packages/testing/nros-tests/fixtures/multi_pkg_workspace_freertos:firmware:firmware:thumbv7m-none-eabi"
    # multi-tier freertos firmware (228.G run_tiers): built from the staged root.
    "orch_tiers_freertos:packages/testing/nros-tests/fixtures/orchestration_tiers_freertos:.:demo_entry:thumbv7m-none-eabi"
)

stage_and_cross_build() {
    local id="$1" src="$2" subdir="$3" pkg="$4" target="$5"
    local staged="$out_root/$id"
    if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
        echo "cross-build: target $target not installed — skipping $id" >&2
        return 0
    fi
    echo "== cross-build: $id ($pkg @ $target) =="
    stage_tree "$id" "$src" "$staged"
    rm -f "$staged/.compile-ok"
    # firmware fixtures read the freertos platform sources + cffi headers from
    # the repo via env (the build.rs codegen + cc compile).
    ( cd "$staged/$subdir" \
        && NROS_PLATFORM_FREERTOS_SRC="$repo_root/packages/core/nros-platform-freertos/src" \
           NROS_PLATFORM_CFFI_INCLUDE="$repo_root/packages/core/nros-platform-api/include" \
           cargo build --target "$target" -p "$pkg" )
    date -u +%Y-%m-%dT%H:%M:%SZ > "$staged/.compile-ok"
    echo "   built $staged/$subdir (target/$target)"
}

for entry in "${COMPILE_CHECK_FIXTURES[@]}"; do
    stage_and_check "${entry%%:*}" "${entry#*:}"
done
for entry in "${BUILD_FIXTURES[@]}"; do
    stage_and_build "${entry%%:*}" "${entry#*:}"
done
for entry in "${CROSS_BUILD_FIXTURES[@]}"; do
    IFS=':' read -r cb_id cb_src cb_sub cb_pkg cb_tgt <<< "$entry"
    stage_and_cross_build "$cb_id" "$cb_src" "$cb_sub" "$cb_pkg" "$cb_tgt"
done
# C++ syntax-only compile-checks (id : snippet.cpp under
# packages/testing/nros-tests/fixtures/cpp_compat_snippets/). `c++ -fsyntax-only`
# the snippet against the nros-cpp / nros-c / compat include set — a compile-only
# proof the public C++ API headers type-check. Stamped into build/compile-check
# (same resolver as the cargo compile-checks).
CXX_SYNTAX_FIXTURES=(
    "declared_node_typed_helpers"
    "rclcpp_node_options"
)
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
    # source include dir: `packages/core/nros-cpp/include/nros/nros_cpp_config_generated.h`
    # is a stub that `#error`s, so if it is searched first the real header
    # (`target/nros-cpp-generated/nros/...`, emitted by nros-cpp's build.rs) is
    # never reached. Prepend the generated dirs.
    local inc=()
    [ -f "$repo_root/target/nros-cpp-generated/nros/nros_cpp_config_generated.h" ] \
        && inc+=(-I "$repo_root/target/nros-cpp-generated")
    [ -f "$repo_root/target/nros-c-generated/nros/nros_config_generated.h" ] \
        && inc+=(-I "$repo_root/target/nros-c-generated")
    inc+=(-I "$repo_root/packages/core/nros-cpp/include"
          -I "$repo_root/packages/core/nros-c/include"
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
    for entry in "${CMAKE_FIXTURES[@]}"; do
        build_cmake_fixture "${entry%%:*}" "${entry#*:}"
        cmake_n=$((cmake_n + 1))
    done
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
    for id in "${CXX_SYNTAX_FIXTURES[@]}"; do
        cxx_syntax_check "$id"
        cxx_n=$((cxx_n + 1))
    done
else
    echo "cxx-syntax: no C++ compiler — skipping" >&2
fi

# cargo-check of an existing example dir for a cross target (id : dir : target).
# Proves an example's `nros::main!()` emit type-checks WITHOUT linking — for
# examples that intentionally don't link standalone (e.g. talker-embassy lacks
# the board memory layout). Stamped into build/compile-check (same resolver).
# Gated on the rust target being installed; absent → no stamp → test skips.
CARGO_CHECK_EXAMPLES=(
    "embassy_main_macro:examples/stm32f4/rust/talker-embassy:thumbv7em-none-eabihf"
)
cargo_check_n=0
for entry in "${CARGO_CHECK_EXAMPLES[@]}"; do
    IFS=':' read -r id dir target <<< "$entry"
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
        cargo_check_n=$((cargo_check_n + 1))
    else
        echo "   cargo-check FAILED for $id (no stamp)" >&2
    fi
done

echo "fixtures built (check=${#COMPILE_CHECK_FIXTURES[@]} build=${#BUILD_FIXTURES[@]} cmake=$cmake_n cxx=$cxx_n cargo-check=$cargo_check_n)."
