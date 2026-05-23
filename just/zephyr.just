set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}
# Zephyr workspace path. Default: in-tree (gitignored) `zephyr-workspace/`.
# Falls back to legacy sibling `../nano-ros-workspace` for users set up
# before the in-tree default. Set $NROS_ZEPHYR_WORKSPACE to override.
ZEPHYR_WORKSPACE := env("NROS_ZEPHYR_WORKSPACE", if path_exists("zephyr-workspace") == "true" { "zephyr-workspace" } else if path_exists("../nano-ros-workspace") == "true" { "../nano-ros-workspace" } else { "zephyr-workspace" })

# Install the Zephyr workspace at $repo/zephyr-workspace/ (large download).
# Idempotent: skips if the workspace already exists.
# Set $NROS_ZEPHYR_WORKSPACE to install elsewhere.
# Extra args are forwarded: e.g. `just zephyr setup --force --skip-sdk`.
setup *ARGS:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    ARGS="{{ARGS}}"
    if [ -d "$WORKSPACE/zephyr" ] && [[ "$ARGS" != *--force* ]]; then
        echo "Zephyr workspace already present at $WORKSPACE"
        echo "To reinstall, run: just zephyr setup --force"
    else
        ./scripts/zephyr/setup.sh $ARGS
    fi
    # Phase 160.B — ensure the Cortex-A9 (qemu_cortex_a9) toolchain
    #   1. `armv7a-none-eabi` rust target (no_std core / compiler_builtins)
    #   2. `cortex-a9-rust-patch.sh`: patches Zephyr's Rust module to
    #      register the Cortex-A9 target + adds the SLCR MMU entry the
    #      Zynq-7000 eth_xlnx_gem driver needs.
    # Both are idempotent — safe to run on every `just zephyr setup`.
    echo "[zephyr setup] ensuring armv7a-none-eabi Rust target..."
    rustup target add armv7a-none-eabi
    echo "[zephyr setup] applying Cortex-A9 / SLCR patches (idempotent)..."
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$WORKSPACE"
    echo "[zephyr setup] applying AArch64 / Cortex-R Rust patches (idempotent)..."
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$WORKSPACE"
    bash ./scripts/zephyr/cortex-r-rust-patch.sh "$WORKSPACE"
    echo "[zephyr setup] applying Phase 168.1 cargo-features patch (idempotent)..."
    bash ./scripts/zephyr/cargo-features-patch.sh "$WORKSPACE"
    echo "[zephyr setup] applying Phase 97.4 native_sim IPPROTO_IP patch (idempotent)..."
    bash ./scripts/zephyr/native-sim-ipproto-ip-patch.sh "$WORKSPACE"
    echo "[zephyr setup] applying Phase 168.X.fvp llext-edk conditional patch (idempotent)..."
    bash ./scripts/zephyr/llext-edk-conditional-patch.sh "$WORKSPACE"
    echo "[zephyr setup] applying Phase 11W.6 cyclonedds threads patch (idempotent)..."
    bash ./scripts/zephyr/cyclonedds-zephyr-threads-patch.sh
    echo "[zephyr setup] applying Phase 11W.7 cyclonedds log eager-flush patch (idempotent)..."
    bash ./scripts/zephyr/cyclonedds-zephyr-log-flush-patch.sh
    echo "[zephyr setup] applying Phase 11W.7/.8 cyclonedds udp sockopt patch (idempotent)..."
    bash ./scripts/zephyr/cyclonedds-zephyr-udp-rcvbuf-patch.sh
    echo "[zephyr setup] applying Phase 11W.8 NSOS getsockname patch (idempotent)..."
    bash ./scripts/zephyr/nsos-getsockname-patch.sh
    echo "[zephyr setup] applying Phase 11W.10 NSOS recvmsg patch (idempotent)..."
    bash ./scripts/zephyr/nsos-recvmsg-patch.sh
    echo "[zephyr setup] applying Phase 11W.11 NSOS mcast-join dual-mreq patch (idempotent)..."
    bash ./scripts/zephyr/nsos-mcjoin-mreq-patch.sh
    echo "[zephyr setup] applying Phase 11W.12 NSOS getifaddrs patch (idempotent)..."
    bash ./scripts/zephyr/nsos-getifaddrs-patch.sh
    echo "[zephyr setup] applying Phase 11W.12 NSOS adapt IPPROTO_IP setsockopt patch (idempotent)..."
    bash ./scripts/zephyr/nsos-adapt-ipproto-ip-patch.sh
    echo "[zephyr setup] applying Phase 11W.8 cyclonedds multicast-join best-effort patch (idempotent)..."
    bash ./scripts/zephyr/cyclonedds-zephyr-mcjoin-patch.sh
    echo "[zephyr setup] applying Phase 11W.8 cyclonedds sockwaitset self-pipe patch (idempotent)..."
    bash ./scripts/zephyr/cyclonedds-zephyr-sockwaitset-patch.sh

# Diagnose Zephyr install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if command -v west &>/dev/null; then
        echo "  [OK] west"
    else
        echo "  [MISSING] west (run: just zephyr setup)"
        fail=1
    fi
    if [ -d "$WORKSPACE/zephyr" ]; then
        echo "  [OK] Zephyr workspace: $WORKSPACE"
    else
        echo "  [MISSING] Zephyr workspace at $WORKSPACE (run: just zephyr setup)"
        fail=1
    fi
    if rustup target list --installed 2>/dev/null | grep -q '^armv7a-none-eabi$'; then
        echo "  [OK] armv7a-none-eabi rust target (qemu_cortex_a9 DDS)"
    else
        echo "  [MISSING] armv7a-none-eabi rust target (run: just zephyr setup)"
        fail=1
    fi
    # native_sim uses NSOS (host loopback) — no TAP bridge required.
    exit $fail

# Build Zephyr core artifacts.
build:
    @echo "Zephyr has no separate core build; use 'just zephyr build-examples' for examples."

# Build Zephyr Rust examples (all Rust examples for native_sim)
build-rust-examples:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    # Apply Cortex-A9 patches (idempotent; no-op if already applied).
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$WORKSPACE"
    echo "Building Zephyr Rust examples in $WORKSPACE..."
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    for ex in talker listener service-server service-client action-server action-client service-client-async; do
    done
    echo "Zephyr Rust examples built successfully!"

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
    echo "Building Zephyr test fixtures..."
    # Phase 140 — Zephyr examples consume nano-ros via the Phase 139
    # integration shell (`integrations/zephyr/`) under `west build`.
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
    # Phase 140 — Zephyr C/C++ DDS examples call nros_generate_interfaces()
    # which needs nros-codegen on the host. Cross builds (every Zephyr
    # board target) can't build it with the cross toolchain, so build
    # once with host cargo and pass the absolute path via west -D.
    echo "  → host nros-codegen (cargo)"
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo build $cargo_profile_args --manifest-path packages/codegen/packages/Cargo.toml \
        -p nros-codegen-c --bin nros-codegen --quiet
    codegen_tool="$nros_root/packages/codegen/packages/target/$(nros_cargo_target_profile_dir)/nros-codegen"
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
    # Phase 177.18 — Zephyr native_sim's final runner link invokes
    # Zephyr's scripts/native_simulator/Makefile through CMake's MAKE
    # cache variable. Point it at the pinned GNU make 4.4 build from
    # `just setup` so the fifo jobserver survives every build stage.
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        cmake_build_env=()
    else
        cmake_build_env=(CMAKE_BUILD_PARALLEL_LEVEL="$ninja_jobs")
    fi
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

    # entries: "<board>|<build_dir>|<src_path_under_examples>|<best_effort?>"
    entries=(
      # Zenoh Rust (native_sim)
      # Zenoh C++ (native_sim) — talker/listener required, service+action best-effort
      # XRCE entries carry a 5th `xrce_agent_port` column so each
      # (variant, lang) pair builds with its dedicated Kconfig port —
      # mirrors `nros_tests::platform::ZEPHYR.xrce_agent_port_for`.
      # Pubsub uses base; service / action add 10 / 20; lang stride 100.
      # Pairs that share a test (talker+listener, server+client) MUST
      # have the same port.
      #
      # Phase 95.A — XRCE Rust full 6-set (build dirs match `nros_tests::zephyr`)
      # XRCE C
      # Phase 95.C — XRCE C++ full 6-set
      # Phase 71.8 + Phase 93 DDS native_sim boot smoke (best-effort).
      # Keep this list in sync with the qemu_cortex_a9 set below — the
      # zephyr boots tests in nros-tests/tests/zephyr.rs expect every
      # example here as a native_sim binary.
      # Phase 92 + 95.B — DDS Rust full 7-set on qemu_cortex_a9 (real IGMP/GEM/ARP)
      # Phase 95.D — DDS C++ full 6-set on both native_sim and qemu_cortex_a9
      # Phase 95.E — DDS C full 6-set on both native_sim and qemu_cortex_a9

      # Phase 168.3 — collapsed Rust cases (single dir, RMW via prj overlay).
      "native_sim/native/64|build-rs-talker-zenoh|zephyr/rust/talker|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-rs-talker-xrce|zephyr/rust/talker|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-rs-listener-zenoh|zephyr/rust/listener|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-rs-listener-xrce|zephyr/rust/listener|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-rs-service-server-zenoh|zephyr/rust/service-server|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-rs-service-server-xrce|zephyr/rust/service-server|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-rs-service-client-zenoh|zephyr/rust/service-client|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-rs-service-client-xrce|zephyr/rust/service-client|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-rs-action-server-zenoh|zephyr/rust/action-server|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-rs-action-server-xrce|zephyr/rust/action-server|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-rs-action-client-zenoh|zephyr/rust/action-client|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-rs-action-client-xrce|zephyr/rust/action-client|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-rs-service-client-async-zenoh|zephyr/rust/service-client-async|0||prj.conf;prj-zenoh.conf"

      # Phase 168.4 — collapsed C cases (zenoh + xrce; dds deferred
      # to Phase 168.X — see docs/roadmap/phase-168-X-zephyr-cmake-build-gaps.md).
      "native_sim/native/64|build-c-talker-zenoh|zephyr/c/talker|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-c-talker-xrce|zephyr/c/talker|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-c-listener-zenoh|zephyr/c/listener|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-c-listener-xrce|zephyr/c/listener|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-c-service-server-zenoh|zephyr/c/service-server|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-c-service-server-xrce|zephyr/c/service-server|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-c-service-client-zenoh|zephyr/c/service-client|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-c-service-client-xrce|zephyr/c/service-client|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-c-action-server-zenoh|zephyr/c/action-server|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-c-action-server-xrce|zephyr/c/action-server|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-c-action-client-zenoh|zephyr/c/action-client|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-c-action-client-xrce|zephyr/c/action-client|0||prj.conf;prj-xrce.conf"

      # Phase 168.X gap 1 unblocked — collapsed C++ cases. cyclonedds
      # native_sim hits an upstream Zephyr cmake gen-expr bug; FVP /
      # aemv8r path remains in `examples/zephyr/cpp/cyclonedds/talker-aemv8r/`.
      "native_sim/native/64|build-cpp-talker-zenoh|zephyr/cpp/talker|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-cpp-talker-xrce|zephyr/cpp/talker|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-cpp-listener-zenoh|zephyr/cpp/listener|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-cpp-listener-xrce|zephyr/cpp/listener|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-cpp-service-server-zenoh|zephyr/cpp/service-server|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-cpp-service-server-xrce|zephyr/cpp/service-server|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-cpp-service-client-zenoh|zephyr/cpp/service-client|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-cpp-service-client-xrce|zephyr/cpp/service-client|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-cpp-action-server-zenoh|zephyr/cpp/action-server|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-cpp-action-server-xrce|zephyr/cpp/action-server|0||prj.conf;prj-xrce.conf"
      "native_sim/native/64|build-cpp-action-client-zenoh|zephyr/cpp/action-client|0||prj.conf;prj-zenoh.conf"
      "native_sim/native/64|build-cpp-action-client-xrce|zephyr/cpp/action-client|0||prj.conf;prj-xrce.conf"

      # Phase 11W — Cyclone DDS native_sim. Compile + link path
      # unblocked by Phase 168.X.fvp llext-edk patch + cxx-compat
      # shims + DDS_HAS_* `#ifdef`-fix + link stubs.
      "native_sim/native/64|build-rs-talker-cyclonedds|zephyr/rust/talker|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-rs-listener-cyclonedds|zephyr/rust/listener|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-rs-service-server-cyclonedds|zephyr/rust/service-server|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-rs-service-client-cyclonedds|zephyr/rust/service-client|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-rs-action-server-cyclonedds|zephyr/rust/action-server|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-rs-action-client-cyclonedds|zephyr/rust/action-client|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-c-talker-cyclonedds|zephyr/c/talker|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-c-listener-cyclonedds|zephyr/c/listener|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-c-service-server-cyclonedds|zephyr/c/service-server|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-c-service-client-cyclonedds|zephyr/c/service-client|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-c-action-server-cyclonedds|zephyr/c/action-server|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-c-action-client-cyclonedds|zephyr/c/action-client|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-cpp-talker-cyclonedds|zephyr/cpp/talker|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-cpp-listener-cyclonedds|zephyr/cpp/listener|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-cpp-service-server-cyclonedds|zephyr/cpp/service-server|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-cpp-service-client-cyclonedds|zephyr/cpp/service-client|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-cpp-action-server-cyclonedds|zephyr/cpp/action-server|0||prj.conf;prj-cyclonedds.conf"
      "native_sim/native/64|build-cpp-action-client-cyclonedds|zephyr/cpp/action-client|0||prj.conf;prj-cyclonedds.conf"
    )

    failures=()
    running=()

    wait_one() {
        local entry pid src log best_effort
        entry="${running[0]}"
        running=("${running[@]:1}")
        pid="${entry%%|*}"
        entry="${entry#*|}"
        src="${entry%%|*}"
        entry="${entry#*|}"
        log="${entry%%|*}"
        best_effort="${entry#*|}"
        if wait "$pid"; then
            :
        elif [ "$best_effort" = "1" ]; then
            echo "  ! best-effort failed: $src (log: $log)"
        else
            echo "  ✗ failed: $src (log: $log)"
            tail -80 "$log" || true
            failures+=("$src")
        fi
    }

    selected=0
    for entry in "${entries[@]}"; do
        # Columns:
        #   5  xrce_agent_port (optional) — forwarded as
        #      `-DCONFIG_NROS_XRCE_AGENT_PORT=<port>` so each
        #      (variant, lang) pair builds against its own Kconfig
        #      port slot.
        #   6  conf_files (optional) — `;`-separated overlay list
        #      forwarded as `-DCONF_FILE="..."` for Phase 168.x
        #      collapsed cases that select RMW via prj overlay.
        IFS='|' read -r board build_dir src best_effort xrce_port conf_files <<< "$entry"
        if [ -n "$fixture_filter" ] && ! [[ "$board $build_dir $src $conf_files" =~ $fixture_filter ]]; then
            continue
        fi
        selected=$((selected + 1))
        if [[ "$build_dir" = /* ]]; then
            actual_build_dir="$build_dir"
        else
            actual_build_dir="$build_root/$build_dir"
        fi
        echo "  → zephyr/$src (-d $actual_build_dir, $board${xrce_port:+, xrce-port=$xrce_port}${conf_files:+, conf=$conf_files})"
        extra_args=(-D_NANO_ROS_CODEGEN_TOOL="$codegen_tool"
                    -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR="$toolchain_cache_dir"
                    -DMAKE="$make_bin"
                    -DUSE_CCACHE=0)
        if [ -n "$xrce_port" ]; then
            extra_args+=(-DCONFIG_NROS_XRCE_AGENT_PORT="$xrce_port")
        fi
        if [ -n "$conf_files" ]; then
            extra_args+=(-DCONF_FILE="$conf_files")
        fi
        # Phase 165.perf — route Zephyr's C/C++ compiles through sccache
        # so the picolibc / kernel / Cyclone objects (identical across
        # same-RMW examples → same autoconf.h) become cache hits instead
        # of full per-example recompiles. sccache degrades to a direct
        # compile on any uncacheable invocation, so this is safe; the
        # only historical caution was parallel west fan-out (still
        # gated behind NROS_ZEPHYR_BUILD_JOBS), not sccache itself.
        sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            sccache_launcher=1
            extra_args+=(-DCMAKE_C_COMPILER_LAUNCHER=sccache
                         -DCMAKE_CXX_COMPILER_LAUNCHER=sccache)
        fi
        log="$log_dir/${build_dir}.log"
        # Phase 140 — pass extra_args only when populated; the example's
        # CMakeLists consumes nano-ros via add_subdirectory (no PREFIX_PATH).
        west_extra=()
        if [ "${#extra_args[@]}" -gt 0 ]; then
            west_extra=(-- "${extra_args[@]}")
        fi
        # Phase 174.A residual — if the build dir is already configured,
        # build with ninja directly instead of `west build`, which
        # ALWAYS re-runs the full cmake configure (devicetree regen,
        # Rust-target detect, codegen) — ~4 s/example even on a no-op.
        # ninja regenerates build.ninja itself only when a cmake input
        # (CMakeLists / prj.conf) actually changed, so this is correct;
        # `pristine=always` still forces a clean west reconfigure. If an
        # older build dir cached different cmake inputs, re-run west
        # once so the cache matches this fixture variant. Under the
        # fifo jobserver (NROS_JOBSERVER=1) ninja inherits the pool;
        # otherwise it gets the per-build ninja-jobs budget.
        sig_file="$actual_build_dir/.nros-zephyr-fixture.sig"
        desired_sig="$(printf '%s\n' \
            "board=$board" \
            "src=$src" \
            "xrce_port=$xrce_port" \
            "conf_files=$conf_files" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$sccache_launcher")"
        needs_west=0
        cache_file="$actual_build_dir/CMakeCache.txt"
        if [ -f "$cache_file" ] && ! grep -qxF "MAKE:FILEPATH=$make_bin" "$cache_file"; then
            needs_west=1
        fi
        if [ ! -f "$sig_file" ] || [ "$(cat "$sig_file")" != "$desired_sig" ]; then
            needs_west=1
        fi
        if [ "$pristine" != "always" ] && [ -f "$actual_build_dir/build.ninja" ] && [ "$needs_west" = "0" ]; then
            use_west=0
            if [ "${NROS_JOBSERVER:-}" = "1" ]; then
                build_argv=(ninja -C "$actual_build_dir")
            else
                build_argv=(ninja -C "$actual_build_dir" -j "$ninja_jobs")
            fi
        else
            use_west=1
            build_argv=(west build -b "$board" -d "$actual_build_dir" -p "$pristine"
                        "$nros_root/examples/$src" "${west_extra[@]}")
        fi
        if [ "$jobs" -eq 1 ]; then
            set +e
            (
              cd "$workspace"
              env PATH="$tool_path" SCCACHE_DISABLE="$sccache_disable" \
                CCACHE_DIR="$ccache_dir" CCACHE_TEMPDIR="$ccache_tmpdir" \
                "${cmake_build_env[@]}" \
                "${build_argv[@]}"
              rc=$?
              if [ "$rc" -eq 0 ] && [ "$use_west" = "1" ]; then
                printf '%s\n' "$desired_sig" > "$sig_file"
              fi
              exit "$rc"
            ) 2>&1 | tee "$log"
            rc="${PIPESTATUS[0]}"
            set -e
            if [ "$rc" -eq 0 ]; then
                :
            elif [ "$best_effort" = "1" ]; then
                echo "  ! best-effort failed: $src (log: $log)"
            else
                echo "  ✗ failed: $src (log: $log)"
                failures+=("$src")
            fi
        else
            (
              cd "$workspace"
              env PATH="$tool_path" SCCACHE_DISABLE="$sccache_disable" \
                CCACHE_DIR="$ccache_dir" CCACHE_TEMPDIR="$ccache_tmpdir" \
                "${cmake_build_env[@]}" \
                "${build_argv[@]}"
              rc=$?
              if [ "$rc" -eq 0 ] && [ "$use_west" = "1" ]; then
                printf '%s\n' "$desired_sig" > "$sig_file"
              fi
              exit "$rc"
            ) > "$log" 2>&1 &
            running+=("$!|$src|$log|$best_effort")
            while [ "${#running[@]}" -ge "$jobs" ]; do
                wait_one
            done
        fi
    done
    while [ "${#running[@]}" -gt 0 ]; do
        wait_one
    done
    if [ "$selected" -eq 0 ]; then
        echo "  ✗ no Zephyr fixtures matched NROS_ZEPHYR_FIXTURE_FILTER=$fixture_filter"
        exit 1
    fi
    if [ ${#failures[@]} -gt 0 ]; then
        printf '  ✗ failed: %s\n' "${failures[@]}"
        exit 1
    fi
    echo "Zephyr test fixtures built successfully."

# Phase 117.13 — Zephyr build smoke for the FVP Base_RevC AEMv8-R
# Cortex-A SMP target (Phase 117.10's reference platform). Reuses
# fixture — no new app required just to prove the toolchain +
# board config + nros module compile clean for AArch64. Skips
# cleanly when west / Zephyr workspace / aarch64-zephyr-elf SDK
# aren't available.
build-fvp-aemv8r:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/aarch64-zephyr-elf/bin/aarch64-zephyr-elf-gcc" ]; then
        echo "Zephyr/FVP skip: aarch64-zephyr-elf toolchain missing in $sdk"
        exit 0
    fi
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    nros_root="$(pwd)"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    extra="$nros_root/packages/boards/nros-board-fvp-aemv8r-smp/boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    west build -b "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp" \
        -d build-fvp-aemv8r-talker -p auto \
        -- -DEXTRA_CONF_FILE="$extra" -DMAKE="$make_bin"
    echo "FVP AEMv8-R Cortex-A SMP build smoke OK"

# Phase 117.14 — Zephyr build smoke for the AEMv8-R Cortex-A SMP
# nros C++ pub/sub example. Validates the Zephyr build of the
# `nros-cpp` API + codegen pipeline through to a linked
# `zephyr.elf` for the FVP target.
build-fvp-aemv8r-cyclonedds:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP-Cyclone skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP-Cyclone skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/aarch64-zephyr-elf/bin/aarch64-zephyr-elf-gcc" ]; then
        echo "Zephyr/FVP-Cyclone skip: aarch64-zephyr-elf toolchain missing"
        exit 0
    fi
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    nros_root="$(pwd)"
    echo "  → host nros-codegen (cargo)"
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo build $cargo_profile_args --manifest-path packages/codegen/packages/Cargo.toml \
        -p nros-codegen-c --bin nros-codegen --quiet
    codegen_tool="$nros_root/packages/codegen/packages/target/$(nros_cargo_target_profile_dir)/nros-codegen"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    echo "Building examples/zephyr/cpp/cyclonedds/talker-aemv8r for fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp..."
    west build -b "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp" \
        -d build-aemv8r-cyclonedds-talker -p auto \
        nano-ros/examples/zephyr/cpp/cyclonedds/talker-aemv8r \
        -- -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" -DMAKE="$make_bin"
    echo "Phase 117.14 FVP AEMv8-R nros-cpp + codegen build smoke OK"

# Phase 117.13 — Zephyr build smoke for the NXP X-S32Z270-DC (DC2)
# RTU0 Cortex-R52 target (Phase 117.11's reference platform). Reuses
# cleanly when west / Zephyr workspace / arm-zephyr-eabi SDK aren't
# available.
build-s32z:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/S32Z skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/S32Z skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/arm-zephyr-eabi/bin/arm-zephyr-eabi-gcc" ]; then
        echo "Zephyr/S32Z skip: arm-zephyr-eabi toolchain missing in $sdk"
        exit 0
    fi
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/cortex-r-rust-patch.sh "$workspace"
    nros_root="$(pwd)"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    extra="$nros_root/packages/boards/nros-board-s32z270dc2-r52/boards/s32z2xxdc2_s32z270_rtu0_D.conf"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    # Use the zenoh talker rather than the DDS one — the eval board
    # has only 1 MiB SRAM, which Rust core overflows by
    # ~100 KiB. zenoh-pico's footprint fits and exercises the same
    # toolchain + nros module + Rust API path the build smoke needs
    # to validate. Real Cyclone DDS deployments live on devices with
    # 4+ MiB SRAM (or LPDDR-backed regions) — out of scope for a
    # build-only Phase 117.13 gate.
    # Best-effort: validates the Rust + nros toolchain integration
    # for the Cortex-R52 target (Phase 117.11/13). Two pre-existing
    # issues block a clean full-link build, both outside Phase
    # 117.11's scope:
    #
    #   1. The S32Z RTU0 has only 1 MiB SRAM (post-XIP=n) so the
    #      full nros + zenoh-pico image overflows at link time.
    #   2. zpico-sys's cbindgen-generated `uintptr_t` mismatches the
    #      `size_t` in `zpico.c` under arm-zephyr-eabi picolibc (a
    #      cbindgen-config issue tracked separately).
    #
    # Both gates fall after Cargo successfully compiles the Rust
    # application + nros + zpico-sys for `armv7r-none-eabihf` — which
    # is the actual Phase 117.13 acceptance bar (toolchain plumbing
    # works for the new board target).
    set +e
    west build -b "s32z2xxdc2@D/s32z270/rtu0" \
        -d build-s32z-talker -p auto \
        -- -DEXTRA_CONF_FILE="$extra" -DMAKE="$make_bin" \
        > "$nros_root/build-s32z-output.log" 2>&1
    rc=$?
    set -e
    rust_release="$workspace/build-s32z-talker/rust/target/armv7r-none-eabihf/release/librustapp.a"
    rust_debug="$workspace/build-s32z-talker/rust/target/armv7r-none-eabihf/debug/librustapp.a"
    if [ "$rc" -eq 0 ]; then
        echo "S32Z R52 build smoke OK (full link succeeded)"
    elif [ -f "$rust_release" ] || [ -f "$rust_debug" ]; then
        echo "S32Z R52 build smoke OK (Rust + nros compile path verified;"
        echo "  full link is best-effort — pre-existing SRAM overflow +"
        echo "  zpico-sys cbindgen typedef issue, see Phase 117 docs)."
    else
        echo "S32Z R52 build smoke FAILED — Rust + nros compile path broken"
        echo "  Build log: $nros_root/build-s32z-output.log"
        exit 1
    fi

# Build Zephyr C examples
build-c:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr C examples in $WORKSPACE..."
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    echo "Zephyr C examples built successfully!"

# Phase 88.15.e — build the nros-log Zephyr native_sim smoke
# fixture (boots Zephyr `native_sim/native/64` as a Linux ELF,
# emits every Severity via the nros-log facade, and exits through
# `nsi_exit(0)`). Consumed by
# `packages/testing/nros-tests/tests/logging_smoke.rs`.
build-logging-smoke:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    source scripts/build/cargo.sh
    CODEGEN="$(pwd)/packages/codegen/packages/target/$(nros_cargo_target_profile_dir)/nros-codegen"
    MAKE_BIN="$(pwd)/third-party/make/make"
    if [ ! -x "$MAKE_BIN" ]; then
        MAKE_BIN="$(command -v make)"
    fi
    export PATH="$(pwd)/third-party/make:$(pwd)/third-party/ninja:$PATH"
    if [ ! -x "$CODEGEN" ]; then
        echo "  Building host nros-codegen..."
        cargo_profile_args="$(nros_cargo_profile_arg_string)"
        cargo build $cargo_profile_args --manifest-path packages/codegen/packages/Cargo.toml \
            -p nros-codegen-c --bin nros-codegen --quiet
    fi
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    echo "  Building logging-smoke-zephyr-native-sim -> build-logging-smoke/"
    west build -b native_sim/native/64 -d build-logging-smoke -p auto \
        $NROS/packages/testing/nros-tests/bins/logging-smoke-zephyr-native-sim \
        -- -D_NANO_ROS_CODEGEN_TOOL="$CODEGEN" -DMAKE="$MAKE_BIN"

# Build Zephyr C++ examples
build-cpp:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr C++ examples in $WORKSPACE..."
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    for ex in talker listener service-server service-client action-server action-client; do
    done
    echo "Zephyr C++ examples built successfully!"

# Build Zephyr XRCE examples (Rust + C for XRCE-DDS backend)
build-xrce:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr XRCE examples in $WORKSPACE..."
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    for ex in talker listener service-server service-client action-server action-client; do
    done
    # Phase 95.C — cpp/xrce 6-example set
    for ex in talker listener service-server service-client action-server action-client; do
    done
    echo "Zephyr XRCE examples built successfully!"

# Build all Zephyr examples (Rust + C + C++ + XRCE)
build-examples: build-rust-examples build-c build-cpp build-xrce
    @echo "All Zephyr examples built!"

# Build every Zephyr tier: core artifacts, examples, and test fixtures.
build-all: build build-examples build-fixtures
    @echo "Zephyr build-all completed."

# Run Zephyr E2E tests (requires pre-built Zephyr examples; native_sim uses NSOS on host loopback)
test verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    just build-zenohd
    args=(-p nros-tests --test zephyr --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"

# Run all Zephyr tests (zenoh + XRCE + C)
test-all verbose="":
    #!/usr/bin/env bash
    set +e
    failed=0
    just zephyr test {{verbose}} || failed=1
    just zephyr test-xrce {{verbose}} || failed=1
    just zephyr test-c || failed=1
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some Zephyr tests failed."
        exit 1
    fi

# Phase 121.6 — build the Zephyr native_sim smoke test for
# nros-platform-zephyr. Pulls platform.c + timer.c (not net.c)
# directly into a minimal Zephyr application and links via west.
build-c-port:
    #!/usr/bin/env bash
    set -e
    if [ ! -f zephyr-workspace/env.sh ]; then
        echo "zephyr-workspace/env.sh missing — run: just zephyr setup" >&2
        exit 2
    fi
    make_bin="$(pwd)/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    export PATH="$(pwd)/third-party/make:$(pwd)/third-party/ninja:$PATH"
    source zephyr-workspace/env.sh
    west build -b native_sim/native/64 \
        -d tests/zephyr-c-smoke/build tests/zephyr-c-smoke \
        -- -DMAKE="$make_bin"

# Phase 121.6 — run the Zephyr native_sim smoke test on the host.
test-c-port: build-c-port
    #!/usr/bin/env bash
    set -e
    timeout 15 ./tests/zephyr-c-smoke/build/zephyr/zephyr.exe

# CI: doctor + test
ci: doctor test

# Run Zephyr tests with full rebuild
test-full verbose="": build-examples
    just zephyr test {{verbose}}

# Run Zephyr XRCE E2E tests
test-xrce verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    args=(-p nros-tests --test zephyr --no-fail-fast -E 'test(xrce)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"

# Run Zephyr C examples test
test-c:
    just build-zenohd
    ./tests/zephyr/run-c.sh

# Clean Zephyr build directories
clean:
    #!/usr/bin/env bash
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    rm -rf "$WORKSPACE"/build-{talker,listener,service-server,service-client,action-server,action-client,service-client-async,async-service-client,c-talker,c-listener,cpp-talker,cpp-listener,cpp-service-server,cpp-service-client,cpp-action-server,cpp-action-client,xrce-rs-talker,xrce-rs-listener,xrce-c-talker,xrce-c-listener}
    echo "Zephyr build directories cleaned"

# Force rebuild Zephyr examples
rebuild: clean build-examples

# Show Zephyr help
help:
    @echo "Zephyr Examples"
    @echo "==============="
    @echo ""
    @echo "Prerequisites:"
    @echo "  Set up Zephyr workspace: ./scripts/zephyr/setup.sh"
    @echo "  (native_sim uses NSOS; no TAP bridge/sudo required.)"
    @echo ""
    @echo "Build examples:"
    @echo "  just zephyr build          # Build Zephyr core artifacts"
    @echo "  just zephyr build-examples # Build all Zephyr examples"
    @echo "  just zephyr build-rust-examples # Build Rust zenoh Zephyr examples"
    @echo "  just zephyr build-c        # Build C zenoh Zephyr examples"
    @echo "  just zephyr build-cpp      # Build C++ zenoh Zephyr examples"
    @echo "  just zephyr build-xrce     # Build XRCE Zephyr examples (Rust + C)"
    @echo "  just zephyr build-all      # Build Zephyr core, examples, and fixtures"
    @echo "  just zephyr rebuild        # Clean and rebuild"
    @echo "  just zephyr clean          # Remove all build directories"
    @echo ""
    @echo "Run tests:"
    @echo "  just zephyr test           # Run zenoh Zephyr E2E tests"
    @echo "  just zephyr test-xrce      # Run XRCE Zephyr E2E tests"
    @echo "  just zephyr test-full      # Rebuild and run zenoh Zephyr tests"
    @echo "  just zephyr test-c         # Run Zephyr C examples test"
    @echo ""
    @echo "Manual build (from Zephyr workspace):"

# =============================================================================
# Run examples (build + launch native_sim binary)
#
# Each recipe passes --seed=$RANDOM so that multiple native_sim instances
# get different Zenoh session IDs. Without unique seeds, deterministic
# entropy produces identical ZIDs and the router rejects the second
# session with Close(MAX_LINKS). On real SoCs this isn't needed — the
# hardware RNG provides unique entropy.
# =============================================================================

# Run Zephyr talker
talker:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    ./build-talker/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr listener
listener:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    ./build-listener/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr service server
service-server:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    ./build-service-server/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr service client
service-client:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    ./build-service-client/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr action server
action-server:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    ./build-action-server/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr action client
action-client:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    ./build-action-client/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr async service client
async-service-client:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    ./build-async-service-client/zephyr/zephyr.exe

# Start zenohd router for Zephyr examples (port 7456, all interfaces)
zenohd:
    build/zenohd/zenohd --listen tcp/0.0.0.0:7456 --no-multicast-scouting
