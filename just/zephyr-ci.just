# Build all example binaries used as test fixtures (Rust + C + C++).
#
# `west build` is ~3 s of pure overhead per example even on a no-op
# rebuild (python startup + cmake reconfigure walk), but broad parallel
# fan-out against one Zephyr workspace has proven racy in practice
# (hung ninja children and follow-up rebuild cascades). Default to one
# west build at a time (NROS_ZEPHYR_BUILD_JOBS=1) — that's the racy
# axis — but each build's ninja runs parallel (NROS_ZEPHYR_NINJA_JOBS
# defaults to nproc/4; intra-build ninja parallelism is standard,
# non-racy Zephyr usage). Raise NROS_ZEPHYR_BUILD_JOBS only when the
# workspace is known to tolerate concurrent west builds.
#
# Phase 165.perf — sccache is now ENABLED by default (it routes the
# C/C++ compiler through sccache via CMAKE_*_COMPILER_LAUNCHER below).
# The repeated cost here is picolibc (~1300 objects) + the Zephyr
# kernel rebuilt fresh per example; same-RMW examples share an
# identical prj overlay → identical autoconf.h → those objects become
# cache hits. sccache falls back to a direct compile on anything
# uncacheable, so it's safe even at the serial default. Set
# `NROS_ZEPHYR_SCCACHE_DISABLE=1` to opt back out.
[group("full-matrix")]
build-fixtures:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    trap 'for pid in $(jobs -pr); do kill "$pid" 2>/dev/null || true; done; wait; exit 130' INT TERM
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr skip: {{ZEPHYR_WORKSPACE}} not set up (run \`just zephyr setup\`)"
        exit 0
    fi
    # Phase 180.B — examples read the ROS interface package dirs from env
    # (no more hardcoded /opt/ros/humble/share/* PKG_DIR). Default to the
    # stock Humble install; override per package if your ROS lives elsewhere.
    export NROS_STD_MSGS_DIR="${NROS_STD_MSGS_DIR:-/opt/ros/humble/share/std_msgs}"
    export NROS_EXAMPLE_INTERFACES_DIR="${NROS_EXAMPLE_INTERFACES_DIR:-/opt/ros/humble/share/example_interfaces}"
    export NROS_BUILTIN_INTERFACES_DIR="${NROS_BUILTIN_INTERFACES_DIR:-/opt/ros/humble/share/builtin_interfaces}"
    export NROS_UNIQUE_IDENTIFIER_MSGS_DIR="${NROS_UNIQUE_IDENTIFIER_MSGS_DIR:-/opt/ros/humble/share/unique_identifier_msgs}"
    export NROS_ACTION_MSGS_DIR="${NROS_ACTION_MSGS_DIR:-/opt/ros/humble/share/action_msgs}"
    # Phase 180.A — 4.4 line: run west via the Python 3.12 venv + host
    # toolchain, and skip the 3.7-only patch set. The proven 4.4 fixture
    # subset is zenoh native_sim, which needs none of these patches (NSOS
    # multicast/recvmsg re-anchors are Tasks 5-6, only the cyclonedds path).
    venvbin="{{ZEPHYR_VENV_BIN}}"
    if [ -n "$venvbin" ]; then
        export PATH="$(realpath "$venvbin"):$PATH"
        export ZEPHYR_TOOLCHAIN_VARIANT=host
    fi
    if [ "{{NROS_ZEPHYR_VERSION}}" != "4.4" ]; then
    # Apply Cortex-A9 patches (idempotent; required for the
    # build-dds-a9-{talker,listener} entries below).
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    # Phase 117.10/13 — add AArch64 Cortex-A/R support to
    # zephyr-lang-rust so the FVP Base_RevC AEMv8-R SMP target
    # (Phase-117 Cyclone DDS reference) builds.
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    # Phase 117.11/13 — add Cortex-R AArch32 (ARMv7-R / ARMv8-R)
    # support to zephyr-lang-rust for the NXP S32Z R52 target.
    bash ./scripts/zephyr/cortex-r-rust-patch.sh "$workspace"
    # Forward each example's per-RMW `EXTRA_CARGO_ARGS` (set by its
    # CMakeLists from the Kconfig RMW overlay) into the cargo build, so a
    # non-default RMW (xrce / cyclonedds) compiles only its own backend
    # instead of the crate's default `rmw-zenoh`. Idempotent.
    bash ./scripts/zephyr/cargo-features-patch.sh "$workspace"
    # Phase 97.4.zephyr-native_sim — patch upstream Zephyr's NSOS
    # to forward `IPPROTO_IP` setsockopt (incl. `IP_ADD_MEMBERSHIP`).
    # Without this the listener can't join `239.255.0.1` host-side
    # and DDS SPDP discovery never closes on `native_sim`.
    bash ./scripts/zephyr/native-sim-ipproto-ip-patch.sh "$workspace"
    # Phase 168.X.fvp — backport upstream Zephyr 4.x's CONFIG_LLEXT_EDK
    # gate as a 3.7-LTS-compatible `if(CONFIG_LLEXT)` wrap. Lets the
    # cyclonedds compile-options closure live alongside host-gcc's
    # minimal toolchain `compiler` property set without tripping the
    # llext-edk gen-expr evaluator.
    bash ./scripts/zephyr/llext-edk-conditional-patch.sh "$workspace"
    fi
    echo "Building Zephyr test fixtures..."
    # Phase 140 — Zephyr examples consume nano-ros via the Phase 139
    # integration shell (`zephyr/`) under `west build`.
    # No CMAKE_PREFIX_PATH override / install-local pre-step needed;
    # `add_subdirectory(<repo-root>)` invokes Corrosion in-tree.
    nros_root="$(pwd)"
    log_dir="$nros_root/build/zephyr-fixtures"
    mkdir -p "$log_dir"
    build_root="${NROS_ZEPHYR_BUILD_ROOT:-$workspace}"
    if [ "$build_root" = "$workspace" ] && [ ! -w "$workspace" ]; then
        build_root="$nros_root/build/zephyr-workspace-builds"
        echo "  workspace not writable; using build root: $build_root"
    fi
    mkdir -p "$build_root"
    toolchain_cache_dir="${NROS_ZEPHYR_TOOLCHAIN_CACHE_DIR:-$nros_root/build/zephyr-cache/ToolchainCapabilityDatabase}"
    mkdir -p "$toolchain_cache_dir"
    ccache_dir="${NROS_ZEPHYR_CCACHE_DIR:-$nros_root/build/zephyr-cache/ccache}"
    ccache_tmpdir="${NROS_ZEPHYR_CCACHE_TEMPDIR:-$nros_root/build/zephyr-cache/ccache-tmp}"
    mkdir -p "$ccache_dir" "$ccache_tmpdir"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    tool_path="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    # Phase 178.J — fail/regenerate Zephyr Rust generated crates before
    # scheduling expensive west builds. Direct `just zephyr build-fixtures`
    # is allowed, so do not rely solely on the root `generate-bindings`
    # prerequisite.
    # Phase 214.J.2 — also force regen when the in-tree trait surface
    # (RosAction etc.) drifts since the cached `generated/`, which the
    # mtime-only check above can miss.
    source scripts/build/codegen-stamp.sh
    echo "  → zephyr Rust generated-dir preflight"
    nros_cli="$(nros_cli_bin)"
    # Phase 214.I.2 — fail-loud prereq guard: the preflight loop below
    # invokes `nros ws sync`, absent from the shipped 0.3.7 release.
    # Probe once; emit `[PREREQ]` + exit 0 if missing.
    nros_require_ws_sync "$nros_cli"
    while IFS= read -r pkg; do
        dir="$(dirname "$pkg")"
        stamp="$dir/build/.nros-ws-sync.stamp"
        # Wipe `generated/` if the trait-surface hash has drifted.
        NROS_REPO_DIR="$(pwd)" nros_codegen_stamp_check_or_wipe "$dir"
        if [ "${NROS_ZEPHYR_GENERATE_RUST_FORCE:-0}" = "1" ] || [ ! -f "$stamp" ] || [ "$pkg" -nt "$stamp" ] || [ ! -d "$dir/generated" ]; then
            echo "    ws sync: $dir"
            mkdir -p "$dir/build"
            NROS_REPO_DIR="$(pwd)" "$nros_cli" ws sync "$dir"
            NROS_REPO_DIR="$(pwd)" nros_codegen_stamp_write "$dir"
            touch "$stamp"
        fi
    done < <(find examples/zephyr/rust -mindepth 2 -maxdepth 2 -name package.xml | sort)
    # Phase 140 — Zephyr C/C++ DDS examples call nros_generate_interfaces()
    # which needs nros-codegen on the host. Cross builds (every Zephyr
    # board target) can't build it with the cross toolchain, so build
    # once with host cargo and pass the absolute path via west -D.
    echo "  → host nros-codegen (cargo)"
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
    # Phase 165.perf — split the NROS_BUILD_JOBS budget into parallel
    # example builds (BUILD_JOBS) × ninja-per-build. Each example uses
    # an isolated `-d <build_dir>`, so concurrent west builds are safe
    # (the historical raciness was a shared default build dir, since
    # fixed). Default BUILD_JOBS = budget/8 so each concurrent build
    # still gets ~8 ninja jobs for picolibc's parallel compile. Override
    # with NROS_ZEPHYR_BUILD_JOBS (set =1 to fall back to serial).
    zephyr_budget="${NROS_BUILD_JOBS:-8}"
    default_build_jobs=$(( zephyr_budget / 8 )); [ "$default_build_jobs" -lt 1 ] && default_build_jobs=1
    jobs="${NROS_ZEPHYR_BUILD_JOBS:-$default_build_jobs}"
    if ! [[ "$jobs" =~ ^[0-9]+$ ]] || [ "$jobs" -lt 1 ]; then
        echo "Invalid NROS_ZEPHYR_BUILD_JOBS=$jobs; expected positive integer" >&2
        exit 2
    fi
    # Phase 174.A — default `auto`, not `always`. Each build dir is
    # variant-unique (`build-<lang>-<ex>-<rmw>`, fixed board + fixed
    # prj overlay), so its cmake config never changes — `auto` only
    # re-pristines when the cmake inputs actually change, otherwise it
    # rebuilds incrementally. `always` wiped + full-rebuilt every
    # example on every `just zephyr build-fixtures`, so repeated runs
    # (dev inner-loop, warm-dir CI) never went incremental. Set
    # NROS_ZEPHYR_PRISTINE=always to force a clean rebuild.
    pristine="${NROS_ZEPHYR_PRISTINE:-auto}"
    case "$pristine" in
        auto|always|never) ;;
        *)
            echo "Invalid NROS_ZEPHYR_PRISTINE=$pristine; expected auto, always, or never" >&2
            exit 2
            ;;
    esac
    # Phase 165.perf — ninja parallelism WITHIN a single west build is
    # safe (standard Zephyr usage); the documented raciness was only
    # ever about multiple simultaneous west builds (BUILD_JOBS>1) on the
    # shared workspace. Default to the global NROS_BUILD_JOBS budget so
    # picolibc's ~1300 objects compile in parallel. Under
    # `build-test-fixtures` the budget is already pre-divided across the
    # concurrent platforms, so this is the per-platform share; run
    # standalone it's the full budget. NROS_ZEPHYR_NINJA_JOBS overrides.
    default_ninja_jobs=$(( zephyr_budget / jobs )); [ "$default_ninja_jobs" -lt 1 ] && default_ninja_jobs=1
    ninja_jobs="${NROS_ZEPHYR_NINJA_JOBS:-$default_ninja_jobs}"
    if ! [[ "$ninja_jobs" =~ ^[0-9]+$ ]] || [ "$ninja_jobs" -lt 1 ]; then
        echo "Invalid NROS_ZEPHYR_NINJA_JOBS=$ninja_jobs; expected positive integer" >&2
        exit 2
    fi
    # Phase 226.G — the Zephyr one-leaf runner owns nested job behavior:
    # fifo jobserver leaves omit `ninja -j` / CMAKE_BUILD_PARALLEL_LEVEL,
    # while ordinary make fallback uses NROS_ZEPHYR_NINJA_JOBS.
    sccache_disable="${NROS_ZEPHYR_SCCACHE_DISABLE:-0}"
    case "$sccache_disable" in
        0|1) ;;
        *)
            echo "Invalid NROS_ZEPHYR_SCCACHE_DISABLE=$sccache_disable; expected 0 or 1" >&2
            exit 2
            ;;
    esac
    fixture_filter="${NROS_ZEPHYR_FIXTURE_FILTER:-}"
    echo "  concurrency: $jobs; ninja-jobs: $ninja_jobs; pristine: $pristine; sccache-disable: $sccache_disable (logs: $log_dir)"
    if [ -n "$fixture_filter" ]; then
        echo "  fixture-filter: $fixture_filter"
    fi

    # Phase 225.P.6 — workspace-Entry (Approach A) prep. Provision the
    # workspace message bindings + system bringup glue from the workspace
    # ROOT (not the per-package `examples/zephyr/rust/*` loop above),
    # mirroring the native lane in scripts/build/workspace-fixtures-build.sh.
    # `nros ws sync` is the platform-agnostic codegen step; `codegen-system`
    # materializes the demo_bringup glue. The west build of the Entry leaf
    # (emitted below via --include-workspace-entry) consumes these.
    if [ -d examples/workspaces/rust/src/zephyr_entry ]; then
        echo "  → workspace-entry prep (nros ws sync + codegen-system)"
        ( cd examples/workspaces/rust
          NROS_REPO_DIR="$nros_root" "$nros_cli" ws sync >/dev/null
          NROS_REPO_DIR="$nros_root" "$nros_cli" codegen-system --bringup src/demo_bringup --out build-fixtures/demo_bringup >/dev/null )
    fi

    zephyr_leaf_args=(
        --zephyr-version "{{NROS_ZEPHYR_VERSION}}"
        --nros-root "$nros_root"
        --build-root "$build_root"
        --codegen-tool "$codegen_tool"
        --make-bin "$make_bin"
        --toolchain-cache-dir "$toolchain_cache_dir"
        --sccache-disable "$sccache_disable"
        --pristine "$pristine"
    )
    if [ -d examples/workspaces/rust/src/zephyr_entry ]; then
        zephyr_leaf_args+=(--include-workspace-entry)
    fi
    if [ -n "$fixture_filter" ]; then
        zephyr_leaf_args+=(--filter "$fixture_filter")
    fi
    normal_records=0
    probe_records="$(mktemp)"
    if scripts/build/zephyr-fixture-leaves.sh --emit records "${zephyr_leaf_args[@]}" >"$probe_records" 2>/dev/null; then
        if [ -s "$probe_records" ]; then
            normal_records=1
        fi
    fi
    rm -f "$probe_records"

    failures=()
    if [ "$normal_records" = "1" ]; then
        echo "  → Zephyr fixture make scheduler"
        NROS_ZEPHYR_WORKSPACE="$workspace" \
        NROS_ZEPHYR_TOOL_PATH="$tool_path" \
        NROS_ZEPHYR_MAKE_BIN="$make_bin" \
        NROS_ZEPHYR_BUILD_JOBS="$jobs" \
        NROS_ZEPHYR_NINJA_JOBS="$ninja_jobs" \
        NROS_ZEPHYR_SCCACHE_DISABLE="$sccache_disable" \
        NROS_ZEPHYR_CCACHE_DIR="$ccache_dir" \
        NROS_ZEPHYR_CCACHE_TEMPDIR="$ccache_tmpdir" \
            scripts/build/zephyr-fixture-make-driver.sh "${zephyr_leaf_args[@]}"
    fi
    # Phase 177.9 — the logging-smoke native_sim fixture is consumed by
    # logging_smoke.rs but lives under packages/testing/.../bins (not
    # examples/), so it's outside the entries loop above. build-all puts
    # zephyr in its fixture fanout, so build it here too — otherwise
    # build-all is not a superset of test-all's fixture needs and
    # logging_smoke_zephyr_native_sim hard-fails "fixture not built".
    # Honor the same filter; a logging-only filter is valid even when
    # the normal Zephyr fixture generator emits no records.
    want_logging=0
    if [ -z "$fixture_filter" ] || [[ "logging-smoke-zephyr-native-sim" =~ $fixture_filter ]]; then
        want_logging=1
    fi
    if [ "$normal_records" = "0" ] && [ "$want_logging" -eq 0 ]; then
        echo "  ✗ no Zephyr fixtures matched NROS_ZEPHYR_FIXTURE_FILTER=$fixture_filter"
        exit 1
    fi
    if [ "$want_logging" -eq 1 ]; then
        # Reuse the standalone recipe so the build dir matches exactly what
        # logging_smoke.rs's fixture helper looks for (test path contract).
        echo "  → logging-smoke-zephyr-native-sim"
        if ! ( cd "$nros_root" && just zephyr build-logging-smoke ); then
            failures+=("logging-smoke-zephyr-native-sim")
        fi
    fi
    if [ ${#failures[@]} -gt 0 ]; then
        printf '  ✗ failed: %s\n' "${failures[@]}"
        exit 1
    fi
    # zephyr west build-fixtures (issue 0041): build the west bringup fixtures the
    # cli_bringup_zephyr / board_import tests consume, so they don't run west at
    # test time. Self-gates on west/ZEPHYR_BASE; non-fatal (a failed build — e.g.
    # board_import without the FVP SDK — leaves no stamp, and the test deselects/skips).
    bash scripts/build/west-fixtures.sh || true
    echo "Zephyr test fixtures built successfully."
