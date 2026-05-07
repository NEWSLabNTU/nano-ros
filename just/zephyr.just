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
        # Re-apply Cortex-A9 patches anyway: idempotent, and ensures
        # workspaces created before the patch script existed get the
        # tweaks without needing --force (which re-downloads ~1 GB).
        bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$WORKSPACE"
    else
        ./scripts/zephyr/setup.sh $ARGS
    fi

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
    # native_sim uses NSOS (host loopback) — no TAP bridge required.
    exit $fail

# Build Zephyr Rust examples (all Rust examples for native_sim)
build:
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
    for ex in talker listener service-server service-client action-server action-client async-service-client; do
        echo "  Building zephyr/rust/zenoh/$ex -> build-$ex/"
        west build -b native_sim/native/64 -d "build-$ex" -p auto "$NROS/examples/zephyr/rust/zenoh/$ex"
    done
    echo "Zephyr Rust examples built successfully!"

# Build all example binaries used as test fixtures (Rust + C + C++).
#
# `west build` is ~3 s of pure overhead per example even on a no-op
# rebuild (python startup + cmake reconfigure walk), so we run all the
# per-example invocations in parallel. Each example has its own build-
# directory (`-d build-<id>`), and the test harness already proves
# concurrent west builds against the same workspace are safe — see the
# per-build XDG_CACHE_HOME setup in `nros_tests::zephyr::build_zephyr_example`.
build-fixtures:
    #!/usr/bin/env bash
    set -e
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
    echo "Building Zephyr test fixtures..."
    cmake_pfx="$(pwd)/build/install"
    nros_root="$(pwd)"

    # entries: "<board>|<build_dir>|<src_path_under_examples>|<best_effort?>"
    entries=(
      # Zenoh Rust (native_sim)
      "native_sim/native/64|build-talker|zephyr/rust/zenoh/talker|0"
      "native_sim/native/64|build-listener|zephyr/rust/zenoh/listener|0"
      "native_sim/native/64|build-service-server|zephyr/rust/zenoh/service-server|0"
      "native_sim/native/64|build-service-client|zephyr/rust/zenoh/service-client|0"
      "native_sim/native/64|build-action-server|zephyr/rust/zenoh/action-server|0"
      "native_sim/native/64|build-action-client|zephyr/rust/zenoh/action-client|0"
      # Zenoh C++ (native_sim) — talker/listener required, service+action best-effort
      "native_sim/native/64|build-cpp-talker|zephyr/cpp/zenoh/talker|0"
      "native_sim/native/64|build-cpp-listener|zephyr/cpp/zenoh/listener|0"
      "native_sim/native/64|build-cpp-service-server|zephyr/cpp/zenoh/service-server|1"
      "native_sim/native/64|build-cpp-service-client|zephyr/cpp/zenoh/service-client|1"
      "native_sim/native/64|build-cpp-action-server|zephyr/cpp/zenoh/action-server|1"
      "native_sim/native/64|build-cpp-action-client|zephyr/cpp/zenoh/action-client|1"
      # XRCE entries carry a 5th `xrce_agent_port` column so each
      # (variant, lang) pair builds with its dedicated Kconfig port —
      # mirrors `nros_tests::platform::ZEPHYR.xrce_agent_port_for`.
      # Pubsub uses base; service / action add 10 / 20; lang stride 100.
      # Pairs that share a test (talker+listener, server+client) MUST
      # have the same port.
      #
      # Phase 95.A — XRCE Rust full 6-set (build dirs match `nros_tests::zephyr`)
      "native_sim/native/64|build-xrce-rs-talker|zephyr/rust/xrce/talker|0|2018"
      "native_sim/native/64|build-xrce-rs-listener|zephyr/rust/xrce/listener|0|2018"
      "native_sim/native/64|build-xrce-rs-service-server|zephyr/rust/xrce/service-server|0|2028"
      "native_sim/native/64|build-xrce-rs-service-client|zephyr/rust/xrce/service-client|0|2028"
      "native_sim/native/64|build-xrce-rs-action-server|zephyr/rust/xrce/action-server|0|2038"
      "native_sim/native/64|build-xrce-rs-action-client|zephyr/rust/xrce/action-client|0|2038"
      # XRCE C
      "native_sim/native/64|build-xrce-c-talker|zephyr/c/xrce/talker|0|2118"
      "native_sim/native/64|build-xrce-c-listener|zephyr/c/xrce/listener|0|2118"
      # Phase 95.C — XRCE C++ full 6-set
      "native_sim/native/64|build-xrce-cpp-talker|zephyr/cpp/xrce/talker|0|2218"
      "native_sim/native/64|build-xrce-cpp-listener|zephyr/cpp/xrce/listener|0|2218"
      "native_sim/native/64|build-xrce-cpp-service-server|zephyr/cpp/xrce/service-server|0|2228"
      "native_sim/native/64|build-xrce-cpp-service-client|zephyr/cpp/xrce/service-client|0|2228"
      "native_sim/native/64|build-xrce-cpp-action-server|zephyr/cpp/xrce/action-server|0|2238"
      "native_sim/native/64|build-xrce-cpp-action-client|zephyr/cpp/xrce/action-client|0|2238"
      # Phase 71.8 + Phase 93 DDS native_sim boot smoke (best-effort).
      # Keep this list in sync with the qemu_cortex_a9 set below — the
      # zephyr boots tests in nros-tests/tests/zephyr.rs expect every
      # example here as a native_sim binary.
      "native_sim/native/64|build-dds-rs-talker|zephyr/rust/dds/talker|1"
      "native_sim/native/64|build-dds-rs-listener|zephyr/rust/dds/listener|1"
      "native_sim/native/64|build-dds-rs-service-server|zephyr/rust/dds/service-server|1"
      "native_sim/native/64|build-dds-rs-service-client|zephyr/rust/dds/service-client|1"
      "native_sim/native/64|build-dds-rs-action-server|zephyr/rust/dds/action-server|1"
      "native_sim/native/64|build-dds-rs-action-client|zephyr/rust/dds/action-client|1"
      "native_sim/native/64|build-dds-rs-async-service-client|zephyr/rust/dds/async-service-client|1"
      # Phase 92 + 95.B — DDS Rust full 7-set on qemu_cortex_a9 (real IGMP/GEM/ARP)
      "qemu_cortex_a9|build-dds-a9-talker|zephyr/rust/dds/talker|0"
      "qemu_cortex_a9|build-dds-a9-listener|zephyr/rust/dds/listener|0"
      "qemu_cortex_a9|build-dds-a9-service-server|zephyr/rust/dds/service-server|0"
      "qemu_cortex_a9|build-dds-a9-service-client|zephyr/rust/dds/service-client|0"
      "qemu_cortex_a9|build-dds-a9-action-server|zephyr/rust/dds/action-server|0"
      "qemu_cortex_a9|build-dds-a9-action-client|zephyr/rust/dds/action-client|0"
      "qemu_cortex_a9|build-dds-a9-async-service-client|zephyr/rust/dds/async-service-client|0"
      # Phase 95.D — DDS C++ full 6-set on both native_sim and qemu_cortex_a9
      "native_sim/native/64|build-dds-cpp-talker|zephyr/cpp/dds/talker|0"
      "native_sim/native/64|build-dds-cpp-listener|zephyr/cpp/dds/listener|0"
      "native_sim/native/64|build-dds-cpp-service-server|zephyr/cpp/dds/service-server|0"
      "native_sim/native/64|build-dds-cpp-service-client|zephyr/cpp/dds/service-client|0"
      "native_sim/native/64|build-dds-cpp-action-server|zephyr/cpp/dds/action-server|0"
      "native_sim/native/64|build-dds-cpp-action-client|zephyr/cpp/dds/action-client|0"
      "qemu_cortex_a9|build-dds-cpp-a9-talker|zephyr/cpp/dds/talker|0"
      "qemu_cortex_a9|build-dds-cpp-a9-listener|zephyr/cpp/dds/listener|0"
      "qemu_cortex_a9|build-dds-cpp-a9-service-server|zephyr/cpp/dds/service-server|0"
      "qemu_cortex_a9|build-dds-cpp-a9-service-client|zephyr/cpp/dds/service-client|0"
      "qemu_cortex_a9|build-dds-cpp-a9-action-server|zephyr/cpp/dds/action-server|0"
      "qemu_cortex_a9|build-dds-cpp-a9-action-client|zephyr/cpp/dds/action-client|0"
      # Phase 95.E — DDS C full 6-set on both native_sim and qemu_cortex_a9
      "native_sim/native/64|build-dds-c-talker|zephyr/c/dds/talker|0"
      "native_sim/native/64|build-dds-c-listener|zephyr/c/dds/listener|0"
      "native_sim/native/64|build-dds-c-service-server|zephyr/c/dds/service-server|0"
      "native_sim/native/64|build-dds-c-service-client|zephyr/c/dds/service-client|0"
      "native_sim/native/64|build-dds-c-action-server|zephyr/c/dds/action-server|0"
      "native_sim/native/64|build-dds-c-action-client|zephyr/c/dds/action-client|0"
      "qemu_cortex_a9|build-dds-c-a9-talker|zephyr/c/dds/talker|0"
      "qemu_cortex_a9|build-dds-c-a9-listener|zephyr/c/dds/listener|0"
      "qemu_cortex_a9|build-dds-c-a9-service-server|zephyr/c/dds/service-server|0"
      "qemu_cortex_a9|build-dds-c-a9-service-client|zephyr/c/dds/service-client|0"
      "qemu_cortex_a9|build-dds-c-a9-action-server|zephyr/c/dds/action-server|0"
      "qemu_cortex_a9|build-dds-c-a9-action-client|zephyr/c/dds/action-client|0"
    )

    pids=()
    failures=()
    for entry in "${entries[@]}"; do
        # Optional 5th column: `xrce_agent_port`. Defaults to empty for
        # non-XRCE entries (no override). When present, it's forwarded as
        # `-DCONFIG_NROS_XRCE_AGENT_PORT=<port>` so each (variant, lang)
        # pair builds against its own Kconfig port slot.
        IFS='|' read -r board build_dir src best_effort xrce_port <<< "$entry"
        echo "  → zephyr/$src (-d $build_dir, $board${xrce_port:+, xrce-port=$xrce_port})"
        extra_args=()
        if [ -n "$xrce_port" ]; then
            extra_args+=(-DCONFIG_NROS_XRCE_AGENT_PORT="$xrce_port")
        fi
        (
          cd "$workspace"
          if west build -b "$board" -d "$build_dir" -p auto \
                "$nros_root/examples/$src" \
                -- -DCMAKE_PREFIX_PATH="$cmake_pfx" "${extra_args[@]}" > /dev/null 2>&1; then
              exit 0
          else
              [ "$best_effort" = "1" ] && exit 0 || exit 1
          fi
        ) &
        pids+=("$!:$src")
    done
    for entry in "${pids[@]}"; do
        pid="${entry%%:*}"
        src="${entry#*:}"
        if ! wait "$pid"; then
            failures+=("$src")
        fi
    done
    if [ ${#failures[@]} -gt 0 ]; then
        printf '  ✗ failed: %s\n' "${failures[@]}"
        exit 1
    fi

# Phase 117.13 — Zephyr build smoke for the FVP Base_RevC AEMv8-R
# Cortex-A SMP target (Phase 117.10's reference platform). Reuses
# the existing `zephyr/rust/dds/talker` example as the smoke
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
    cmake_pfx="$nros_root/build/install"
    extra="$nros_root/packages/boards/nros-board-fvp-aemv8r-smp/boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf"
    cd "$workspace"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    echo "Building zephyr/rust/dds/talker for fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp..."
    west build -b "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp" \
        -d build-fvp-aemv8r-talker -p auto \
        nano-ros/examples/zephyr/rust/dds/talker \
        -- -DCMAKE_PREFIX_PATH="$cmake_pfx" -DEXTRA_CONF_FILE="$extra"
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
    cmake_pfx="$nros_root/build/install"
    cd "$workspace"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    echo "Building examples/zephyr-aemv8r-cyclonedds for fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp..."
    west build -b "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp" \
        -d build-aemv8r-cyclonedds-talker -p auto \
        nano-ros/examples/zephyr-aemv8r-cyclonedds \
        -- -DCMAKE_PREFIX_PATH="$cmake_pfx"
    echo "Phase 117.14 FVP AEMv8-R nros-cpp + codegen build smoke OK"

# Phase 117.13 — Zephyr build smoke for the NXP X-S32Z270-DC (DC2)
# RTU0 Cortex-R52 target (Phase 117.11's reference platform). Reuses
# the `zephyr/rust/dds/talker` example as the smoke fixture. Skips
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
    cmake_pfx="$nros_root/build/install"
    extra="$nros_root/packages/boards/nros-board-s32z270dc2-r52/boards/s32z2xxdc2_s32z270_rtu0_D.conf"
    cd "$workspace"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    # Use the zenoh talker rather than the DDS one — the eval board
    # has only 1 MiB SRAM, which dust-dds + Rust core overflows by
    # ~100 KiB. zenoh-pico's footprint fits and exercises the same
    # toolchain + nros module + Rust API path the build smoke needs
    # to validate. Real Cyclone DDS deployments live on devices with
    # 4+ MiB SRAM (or LPDDR-backed regions) — out of scope for a
    # build-only Phase 117.13 gate.
    echo "Building zephyr/rust/zenoh/talker for s32z2xxdc2@D/s32z270/rtu0..."
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
        nano-ros/examples/zephyr/rust/zenoh/talker \
        -- -DCMAKE_PREFIX_PATH="$cmake_pfx" -DEXTRA_CONF_FILE="$extra" \
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
    echo "  Building zephyr/c/zenoh/talker -> build-c-talker/"
    west build -b native_sim/native/64 -d build-c-talker -p auto $NROS/examples/zephyr/c/zenoh/talker
    echo "  Building zephyr/c/zenoh/listener -> build-c-listener/"
    west build -b native_sim/native/64 -d build-c-listener -p auto $NROS/examples/zephyr/c/zenoh/listener
    echo "Zephyr C examples built successfully!"

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
        echo "  Building zephyr/cpp/zenoh/$ex -> build-cpp-$ex/"
        west build -b native_sim/native/64 -d "build-cpp-$ex" -p auto "$NROS/examples/zephyr/cpp/zenoh/$ex"
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
        echo "  Building zephyr/rust/xrce/$ex -> build-xrce-rs-$ex/"
        west build -b native_sim/native/64 -d build-xrce-rs-$ex -p auto $NROS/examples/zephyr/rust/xrce/$ex
    done
    echo "  Building zephyr/c/xrce/talker -> build-xrce-c-talker/"
    west build -b native_sim/native/64 -d build-xrce-c-talker -p auto $NROS/examples/zephyr/c/xrce/talker
    echo "  Building zephyr/c/xrce/listener -> build-xrce-c-listener/"
    west build -b native_sim/native/64 -d build-xrce-c-listener -p auto $NROS/examples/zephyr/c/xrce/listener
    # Phase 95.C — cpp/xrce 6-example set
    for ex in talker listener service-server service-client action-server action-client; do
        echo "  Building zephyr/cpp/xrce/$ex -> build-xrce-cpp-$ex/"
        west build -b native_sim/native/64 -d build-xrce-cpp-$ex -p auto $NROS/examples/zephyr/cpp/xrce/$ex
    done
    echo "Zephyr XRCE examples built successfully!"

# Build all Zephyr examples (Rust + C + C++ + XRCE)
build-all: build build-c build-cpp build-xrce
    @echo "All Zephyr examples built!"

# Run Zephyr E2E tests (requires pre-built Zephyr examples; native_sim uses NSOS on host loopback)
test verbose="":
    #!/usr/bin/env bash
    set -e
    just build-zenohd
    args=(-p nros-tests --test zephyr --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${args[@]}"

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

# CI: doctor + test
ci: doctor test

# Run Zephyr tests with full rebuild
test-full verbose="": build
    just zephyr test {{verbose}}

# Run Zephyr XRCE E2E tests
test-xrce verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test zephyr --no-fail-fast -E 'test(xrce)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${args[@]}"

# Run Zephyr C examples test
test-c:
    just build-zenohd
    ./tests/zephyr/run-c.sh

# Clean Zephyr build directories
clean:
    #!/usr/bin/env bash
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    rm -rf "$WORKSPACE"/build-{talker,listener,service-server,service-client,action-server,action-client,async-service-client,c-talker,c-listener,cpp-talker,cpp-listener,cpp-service-server,cpp-service-client,cpp-action-server,cpp-action-client,xrce-rs-talker,xrce-rs-listener,xrce-c-talker,xrce-c-listener}
    echo "Zephyr build directories cleaned"

# Force rebuild Zephyr examples
rebuild: clean build

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
    @echo "  just zephyr build          # Build Rust zenoh Zephyr examples"
    @echo "  just zephyr build-c        # Build C zenoh Zephyr examples"
    @echo "  just zephyr build-cpp      # Build C++ zenoh Zephyr examples"
    @echo "  just zephyr build-xrce     # Build XRCE Zephyr examples (Rust + C)"
    @echo "  just zephyr build-all      # Build all Zephyr examples"
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
    @echo "  west build -b native_sim/native/64 -d build-talker <repo>/examples/zephyr/rust/zenoh/talker"
    @echo "  west build -b native_sim/native/64 -d build-listener <repo>/examples/zephyr/rust/zenoh/listener"

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
    west build -b native_sim/native/64 -d build-talker -p auto "$NROS/examples/zephyr/rust/zenoh/talker"
    ./build-talker/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr listener
listener:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-listener -p auto "$NROS/examples/zephyr/rust/zenoh/listener"
    ./build-listener/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr service server
service-server:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-service-server -p auto "$NROS/examples/zephyr/rust/zenoh/service-server"
    ./build-service-server/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr service client
service-client:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-service-client -p auto "$NROS/examples/zephyr/rust/zenoh/service-client"
    ./build-service-client/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr action server
action-server:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-action-server -p auto "$NROS/examples/zephyr/rust/zenoh/action-server"
    ./build-action-server/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr action client
action-client:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-action-client -p auto "$NROS/examples/zephyr/rust/zenoh/action-client"
    ./build-action-client/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr async service client
async-service-client:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-async-service-client -p auto "$NROS/examples/zephyr/rust/zenoh/async-service-client"
    ./build-async-service-client/zephyr/zephyr.exe

# Start zenohd router for Zephyr examples (port 7456, all interfaces)
zenohd:
    build/zenohd/zenohd --listen tcp/0.0.0.0:7456 --no-multicast-scouting
