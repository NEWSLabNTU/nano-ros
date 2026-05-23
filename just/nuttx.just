set working-directory := '..'

# Phase 143 — patched qemu-system-arm built by `just qemu setup-qemu`.
# Used by NuttX QEMU recipes so the `-netdev dgram,local.type=unix,…`
# multi-instance path works on Ubuntu jammy (system qemu 6.2 too old).
QEMU_BIN := absolute_path("build/qemu") / "bin/qemu-system-arm"

default:
    @just --list --justfile {{source_file()}}

# Phase 140 — `install` recipe deleted along with the install-local
# layout. NuttX examples consume nano-ros via `add_subdirectory(<repo>)`
# (Phase 144.6) or the `integrations/nuttx/` shell (Phase 139).

# Build NuttX kernel (requires arm-none-eabi-gcc + kconfig tools)
build-kernel:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "Skipping NuttX kernel build (NuttX not found). Run: just nuttx setup"
        exit 0
    fi
    if ! command -v arm-none-eabi-gcc &>/dev/null; then
        echo "Skipping NuttX kernel build (arm-none-eabi-gcc not found)"
        exit 0
    fi
    if ! command -v kconfig-conf &>/dev/null && ! command -v olddefconfig &>/dev/null; then
        echo "Skipping NuttX kernel build (kconfig tools not found)"
        exit 0
    fi
    NUTTX_DIR=$NUTTX_DIR NUTTX_APPS_DIR=$NUTTX_APPS_DIR \
        packages/boards/nros-board-nuttx-qemu-arm/scripts/build-nuttx.sh

# Build NuttX core artifacts.
build: build-kernel
    @echo "NuttX core artifacts built."

# Build NuttX QEMU ARM virt examples
build-examples:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "Skipping NuttX QEMU examples (NuttX not found). Run: just nuttx setup"
        exit 0
    fi
    echo "Building NuttX QEMU ARM virt examples..."
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        # No +nightly override — the nightly version is pinned by
        # examples/qemu-arm-nuttx/rust-toolchain.toml (see comments there).
        (cd examples/qemu-arm-nuttx/rust/$example && cargo build $cargo_profile_args)
    done
    echo "NuttX QEMU examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
build-fixtures:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    source scripts/build/fixture-matrix.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    cargo_frontends="$(nros_cargo_frontend_jobs)"
    cmake_frontends="$(nros_cmake_frontend_jobs)"
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "NuttX skip: arm-none-eabi-gcc not found"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        echo "NuttX skip: NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    echo "Building NuttX test fixtures..."
    # Phase 176 — cmake parallelism via the fifo jobserver when present.
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    # Phase 160 follow-up — parallel cargo builds (per-example isolated
    # target/ dirs, no cross-fixture lock contention).
    rust_dirs=()
    for ex in $(nros_fixture_roles); do
        rust_dirs+=("examples/qemu-arm-nuttx/rust/$ex")
    done
    # Phase 169 retired dust-dds; the NuttX `rust/dds/<ex>` examples had
    # no Cyclone replacement (Cyclone needs a ddsrt NuttX port — see
    # phase-175) and are no longer built.
    # Phase 88.15.c — nros-log severity-coverage smoke fixture.
    rust_dirs+=("packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm")
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        for dir in "${rust_dirs[@]}"; do
            echo "  → $dir"
            ( cd "$dir" && CC_armv7a_nuttx_eabi=arm-none-eabi-gcc \
              cargo build $cargo_profile_args --quiet )
        done
    else
        printf '%s\n' "${rust_dirs[@]}" | parallel --jobs "$cargo_frontends" --halt now,fail=1 --line-buffer \
            'echo "  → {}" && cd {} && CC_armv7a_nuttx_eabi=arm-none-eabi-gcc \
             cargo build $cargo_profile_args --quiet'
    fi
    # Phase 160.D — NuttX C/C++ examples now build via cmake. The
    # tier-3 nros-c cross-Corrosion problem (no `-Z build-std`) is
    # sidestepped by:
    #   - root CMakeLists.txt skips `add_subdirectory(packages/core/nros-c)`
    #     for NuttX (no Corrosion attempt against armv7a-nuttx-eabihf).
    #   - root CMakeLists.txt's NanoRos INTERFACE branch (when
    #     nros_c-static is absent) exposes the public include dirs
    #     directly + sets `NROS_PLATFORM_NUTTX` compile-def.
    #   - `nros-c/include/nros/nros_config_generated.h`'s
    #     `#if defined(NROS_PLATFORM_NUTTX)` arm falls through to
    #     the baked-once `nros_config_generated_nuttx.h` header
    #     (Phase 159 Path C). Sizes are pre-probed for the canonical
    #     NuttX armv7a-eabihf target.
    # Host nros-codegen is still required for `nros_generate_interfaces()`.
    echo "  → host nros-codegen (cargo)"
    cargo build $cargo_profile_args --manifest-path packages/codegen/packages/Cargo.toml \
        -p nros-codegen-c --bin nros-codegen --quiet
    codegen="$(pwd)/packages/codegen/packages/target/$(nros_cargo_target_profile_dir)/nros-codegen"
    toolchain="$(pwd)/cmake/toolchain/armv7a-nuttx-eabi.cmake"
    nuttx_ffi="$(pwd)/packages/boards/nros-board-nuttx-qemu-arm/nros-nuttx-ffi"
    # Phase 118.E — collapsed-shape NuttX C / C++ examples (Zenoh only;
    # Cyclone on NuttX is gated on a hosted socket/runtime port).
    cmake_dirs=()
    for lang in c cpp; do
        for ex in $(nros_fixture_roles); do
            cmake_dirs+=("examples/qemu-arm-nuttx/$lang/$ex")
        done
    done
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        for dir in "${cmake_dirs[@]}"; do
            echo "  → $dir (-DNROS_RMW=zenoh, build-zenoh/)"
            rm -rf "$dir/build-zenoh"
            cmake -S "$dir" -B "$dir/build-zenoh" \
              -DNROS_RMW=zenoh \
              -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
              -DNUTTX_DIR="$nuttx_dir" \
              -DNUTTX_FFI_CRATE_DIR="$nuttx_ffi" \
              -DCMAKE_BUILD_TYPE=Release \
              -D_NANO_ROS_CODEGEN_TOOL="$codegen" > /dev/null
            cmake --build "$dir/build-zenoh" > /dev/null
        done
    else
        printf '%s\n' "${cmake_dirs[@]}" | parallel --jobs "$cmake_frontends" --halt now,fail=1 --line-buffer \
            "echo '  → {} (-DNROS_RMW=zenoh, build-zenoh/)' && rm -rf {}/build-zenoh && \
             cmake -S {} -B {}/build-zenoh \
               -DNROS_RMW=zenoh \
               -DCMAKE_TOOLCHAIN_FILE='$toolchain' \
               -DNUTTX_DIR='$nuttx_dir' \
               -DNUTTX_FFI_CRATE_DIR='$nuttx_ffi' \
               -DCMAKE_BUILD_TYPE=Release \
               -D_NANO_ROS_CODEGEN_TOOL='$codegen' > /dev/null && \
             cmake --build {}/build-zenoh > /dev/null"
    fi

# Phase 157.C — stage the integration shell + every C/C++ example
# under $NUTTX_APPS_DIR/external/ as canonical NuttX external apps,
# then run `make` from the NuttX kernel tree. Exercises
# apps/external/*/Make.defs + apps/external/*/Kconfig discovery
# (the path cmake fixtures `build-fixtures` bypass).
#
# Phase 157.C.17 — recipe owns the NuttX configure step. If
# `.config` is missing OR lacks `CONFIG_NET=y` (stock
# qemu-armv7a/nsh defconfig), copies in
# `packages/boards/nros-board-nuttx-qemu-arm/nuttx-config/defconfig`
# which already has the full networking + TLS_NELEM + virtio-net
# stack zenoh-pico needs. Same defconfig the cmake bring-up uses.
build-fixtures-make:
    #!/usr/bin/env bash
    set -e
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "NuttX skip: arm-none-eabi-gcc not found"; exit 0
    fi
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    nuttx_apps_dir="${NUTTX_APPS_DIR:-$(pwd)/third-party/nuttx/nuttx-apps}"
    if [ ! -d "$nuttx_dir/arch" ]; then
        echo "NuttX skip: NUTTX_DIR=$nuttx_dir not found"; exit 0
    fi
    if [ ! -d "$nuttx_apps_dir" ]; then
        echo "NuttX skip: NUTTX_APPS_DIR=$nuttx_apps_dir not found"; exit 0
    fi
    if ! command -v kconfig-tweak >/dev/null 2>&1; then
        echo "NuttX skip: kconfig-tweak not on PATH (install kconfig-frontends)"; exit 0
    fi
    # Phase 157.C.17 — auto-configure NuttX with the nano-ros
    # board defconfig if `.config` is missing or lacks networking.
    # Networking is mandatory: zenoh-pico links bind/sendto/etc.
    nros_defconfig="$(pwd)/packages/boards/nros-board-nuttx-qemu-arm/nuttx-config/defconfig"
    if [ ! -f "$nuttx_dir/.config" ] || ! grep -q "^CONFIG_NET=y" "$nuttx_dir/.config"; then
        if [ ! -f "$nros_defconfig" ]; then
            echo "NuttX skip: $nros_defconfig missing — can't auto-configure" ; exit 0
        fi
        echo "Configuring NuttX with nano-ros defconfig (CONFIG_NET=y + virtio-net + TLS_NELEM)..."
        cp "$nros_defconfig" "$nuttx_dir/.config"
        ( cd "$nuttx_dir" && make olddefconfig >/dev/null )
    fi
    echo "Staging nano-ros external apps into $nuttx_apps_dir/external/..."
    scripts/nuttx/stage-external-apps.sh "$nuttx_apps_dir"
    # Force apps/Kconfig regeneration so mkkconfig.sh picks up the
    # newly-staged apps/external/Kconfig.
    rm -f "$nuttx_apps_dir/Kconfig"
    # First pass: olddefconfig from kernel. Drives apps preconfig
    # (regenerates apps/Kconfig with the new NROS_* symbols), then
    # drops NROS_* from .config because they're new symbols with
    # default n.
    echo "Pass 1/2: regenerate apps/Kconfig + propagate new symbols"
    ( cd "$nuttx_dir" && make olddefconfig >/dev/null )
    # Now enable every nano-ros knob in .config.
    # Phase 157.C — C-only for now. C++ examples need a parallel
    # nros-cpp Cargo build in the integration shell + symbol-export
    # work tracked as 157.C.11.
    config="$nuttx_dir/.config"
    kconfig-tweak --file "$config" --enable NROS
    kconfig-tweak --file "$config" --enable NROS_C_API
    kconfig-tweak --file "$config" --enable NROS_CPP_API
    for lang in C CPP; do
        for ex in TALKER LISTENER SERVICE_SERVER SERVICE_CLIENT ACTION_SERVER ACTION_CLIENT; do
            kconfig-tweak --file "$config" --enable "NROS_EXAMPLE_${ex}_${lang}"
        done
    done
    # Phase 157.C — disable ALLSYMS for first build (chicken-egg:
    # CONFIG_ALLSYMS=y runs mkallsyms.py on $(NUTTX) which doesn't
    # exist on first link). NuttX's stock qemu-armv7a/nsh enables
    # it by default. Carve out as 157.C.12 — multi-pass build that
    # bootstraps allsyms.
    kconfig-tweak --file "$config" --disable ALLSYMS
    # Phase 157.C.11 — Rust std references `pthread_{key_create,
    # key_delete,getspecific,setspecific}` for thread-local
    # storage. NuttX gates these on CONFIG_TLS_NELEM > 0 (stock
    # qemu-armv7a/nsh sets it to 0). Set 8 — enough for std's
    # internal TLS slots + a few user-side keys.
    kconfig-tweak --file "$config" --set-val TLS_NELEM 8
    # Phase 157.C.16 — nros-cpp's `result.hpp` `#include <utility>`
    # (std::move / std::forward). Stock NuttX qemu-armv7a/nsh sets
    # CONFIG_LIBCXXNONE=y (no C++ stdlib headers). Switch to
    # LIBCXXTOOLCHAIN so the toolchain's `<utility>` etc. are
    # available — no extra source build required.
    kconfig-tweak --file "$config" --disable LIBCXXNONE
    kconfig-tweak --file "$config" --enable LIBCXXTOOLCHAIN
    # Pass 2: olddefconfig again so the just-enabled knobs settle
    # any newly-visible dependencies.
    echo "Pass 2/2: olddefconfig with NROS enabled"
    ( cd "$nuttx_dir" && make olddefconfig >/dev/null )
    # Clean per-example app build state so ar re-archives objects
    # into libapps.a. Without this, stale .built timestamps from a
    # prior run convince apps/Application.mk that nothing's stale,
    # but apps/libapps.a (gone after distclean / re-config) lacks
    # the example objects → kernel link fails with undefined
    # `<PROGNAME>_main` references. Skip the integration shell —
    # its `clean::` rule runs `cargo clean` which would wipe the
    # 28 GiB cargo target dir (forcing a full nros-c + nros-cpp
    # rebuild on every recipe invocation).
    for ex in "$nuttx_apps_dir"/external/nano-ros-*; do
        ( cd "$ex" && make clean APPDIR="$nuttx_apps_dir" TOPDIR="$nuttx_dir" >/dev/null 2>&1 || true )
    done
    # Phase 157.C.15 — also force the integration shell to
    # rebuild its CSRCS (platform_wake_stubs.c). Touching the
    # .built marker removes the staleness flag without running
    # the heavy `cargo clean` from the shell's `clean::` rule.
    rm -f "$nuttx_apps_dir"/external/nano-ros/.built \
          "$nuttx_apps_dir"/external/nano-ros/c/*.o
    # Final build. Single binary contains all 12 examples as
    # built-in commands (nuttx_c_talker / nuttx_cpp_listener / ...).
    echo "Building NuttX with all nano-ros examples enabled..."
    ( cd "$nuttx_dir" && make )
    echo "NuttX external-app build complete. Binary: $nuttx_dir/nuttx"

# Build every NuttX tier: core artifacts, examples, and test fixtures.
# Phase 157.C — `build-fixtures-make` runs alongside `build-fixtures`
# (cmake smoke) so both paths exercise: cmake smoke catches QEMU /
# Rust shim regressions fast, make path catches NuttX Kconfig /
# Application.mk / EXTRA_LIBS integration bugs the cmake path can't.
build-all: build build-examples build-fixtures build-fixtures-make
    @echo "NuttX build-all completed."

# Run NuttX QEMU integration tests
test verbose="": build-fixtures
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "ERROR: NuttX not found at $NUTTX_DIR. Run: just nuttx setup"
        exit 1
    fi
    just _nextest-platform nuttx_qemu '{{verbose}}'

# Run all NuttX tests including networked E2E
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    args=(-p nros-tests --test nuttx_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"
    e2e_args=(-p nros-tests --test rtos_e2e -E 'test(Nuttx)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        e2e_args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${e2e_args[@]}"

# CI: check prerequisites + test
ci: doctor test

# Install NuttX prerequisites: initialize submodules + build kernel.
# Phase 123.A.8 — submodule fetch delegated to tools/setup.sh; kernel
# build stays here (NuttX-specific make invocation).
setup:
    #!/usr/bin/env bash
    set -e
    "{{justfile_directory()}}/tools/setup.sh" --platform=nuttx --skip-apt-check
    just nuttx build-kernel
    # Phase 166.E — stage integration shell + example apps under
    # $NUTTX_APPS_DIR/external/ so users following the book's NuttX
    # starter find the example apps under
    # `menuconfig → Application Configuration → External Modules`.
    nuttx_apps_dir="${NUTTX_APPS_DIR:-$(pwd)/third-party/nuttx/nuttx-apps}"
    if [ -f "$nuttx_apps_dir/Make.defs" ]; then
        echo "Staging nano-ros external apps into $nuttx_apps_dir/external/..."
        scripts/nuttx/stage-external-apps.sh "$nuttx_apps_dir"
    else
        echo "Skipping external-app staging: $nuttx_apps_dir/Make.defs not found"
    fi
    echo "NuttX setup complete!"

# Diagnose NuttX install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -d "$NUTTX_DIR/arch" ]; then
        echo "  [OK] NuttX source: $NUTTX_DIR"
    else
        echo "  [MISSING] NuttX source at $NUTTX_DIR (run: just nuttx setup)"
        fail=1
    fi
    if [ -d "$NUTTX_APPS_DIR" ]; then
        echo "  [OK] NuttX apps: $NUTTX_APPS_DIR"
    else
        echo "  [MISSING] NuttX apps at $NUTTX_APPS_DIR (run: just nuttx setup)"
        fail=1
    fi
    if [ -f "$NUTTX_DIR/include/nuttx/config.h" ]; then
        echo "  [OK] NuttX kernel configured"
    else
        echo "  [MISSING] NuttX kernel config ($NUTTX_DIR/include/nuttx/config.h) (run: just nuttx setup)"
        fail=1
    fi
    if command -v arm-none-eabi-gcc &>/dev/null; then
        echo "  [OK] arm-none-eabi-gcc"
    else
        echo "  [MISSING] arm-none-eabi-gcc (run: just workspace apt-packages)"
        fail=1
    fi
    if command -v kconfig-conf &>/dev/null || command -v olddefconfig &>/dev/null; then
        echo "  [OK] kconfig tools"
    else
        echo "  [MISSING] kconfig-frontends-nox (run: just workspace apt-packages)"
        fail=1
    fi
    # Phase 127.B.5 — NuttX DDS RTPS multi-instance tests rely on
    # mcast-style cross-process delivery between two QEMU processes.
    # QEMU 6.2's `socket,mcast=` cannot do this reliably; the
    # documented workaround uses `-netdev dgram,local.type=unix,...`,
    # which was introduced in QEMU 7.2.
    # Phase 143 — patched build at {{QEMU_BIN}} is the primary path
    # picked up automatically by `nros_tests::qemu::qemu_system_arm_path`.
    if [ -x {{QEMU_BIN}} ]; then
        ver=$({{QEMU_BIN}} --version 2>/dev/null | head -1)
        echo "  [OK] patched qemu-system-arm: {{QEMU_BIN}} ($ver)"
    elif command -v qemu-system-arm &>/dev/null; then
        ver=$(qemu-system-arm --version 2>/dev/null | head -1 | sed -E 's/^[^0-9]*([0-9]+\.[0-9]+).*/\1/')
        major=${ver%%.*}
        minor=${ver##*.}
        if [ -n "$ver" ] && { [ "$major" -gt 7 ] || { [ "$major" -eq 7 ] && [ "$minor" -ge 2 ]; }; }; then
            echo "  [OK] system qemu-system-arm ($ver, fallback path; >= 7.2 — supports -netdev dgram unix)"
        else
            echo "  [WARN] system qemu-system-arm ($ver) < 7.2 AND patched build absent —"
            echo "         NuttX DDS multi-instance tests need '-netdev dgram,local.type=unix,...'"
            echo "         which was introduced in QEMU 7.2."
            echo ""
            echo "         Primary remedy (no sudo, portable): just qemu setup-qemu"
            echo "         Fallback (system-wide, requires sudo):"
            if [ -f /etc/os-release ] && grep -q '^ID=ubuntu' /etc/os-release; then
                codename=$(. /etc/os-release; echo "$VERSION_CODENAME")
                echo "           Ubuntu detected ($codename). Canonical server-backports PPA:"
                echo "             sudo add-apt-repository ppa:canonical-server/server-backports"
                echo "             sudo apt update && sudo apt install qemu-system-arm"
                echo "           https://launchpad.net/~canonical-server/+archive/ubuntu/server-backports"
            else
                echo "           Build from source: https://www.qemu.org/download/#source"
            fi
        fi
    else
        echo "  [MISSING] qemu-system-arm — run \`just qemu setup-qemu\` (recommended)"
        echo "            or \`just workspace apt-packages\` for the system binary."
    fi
    exit $fail

# =============================================================================
# Run examples (build + launch QEMU ARM virt with slirp networking)
#
# NOTE: NuttX interactive runs are experimental. The zenoh-pico session may
# fail to connect on first boot. The automated E2E tests (just nuttx test)
# handle this with proper timing. For interactive use, start zenohd first
# and allow a few retries.
#
# QEMU ignores SIGINT — each recipe runs QEMU in the background and
# traps INT/TERM to kill it explicitly (Ctrl-C works).
# =============================================================================

[no-exit-message]
_run-qemu example binary:
    #!/usr/bin/env bash
    set -e
    cd "examples/qemu-arm-nuttx/rust/{{example}}"
    # Nightly version pinned by examples/qemu-arm-nuttx/rust-toolchain.toml.
    CC_armv7a_nuttx_eabihf=arm-none-eabi-gcc cargo build --release
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    # -icount shift=auto: sync virtual clock with wall-clock for proper sleep/timeout
    # -device virtio-net-device: MMIO transport (NuttX virtio-net driver)
    {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -M virt -cpu cortex-a7 -nographic \
        -icount shift=auto \
        -kernel "target/armv7a-nuttx-eabihf/release/{{binary}}" \
        -netdev user,id=net0 -device virtio-net-device,netdev=net0 &
    PID=$!
    wait $PID

# Run NuttX talker on QEMU
talker: (_run-qemu "talker" "nuttx-rs-talker")

# Run NuttX listener on QEMU
listener: (_run-qemu "listener" "nuttx-rs-listener")

# Run NuttX service server on QEMU
service-server: (_run-qemu "service-server" "nuttx-rs-service-server")

# Run NuttX service client on QEMU
service-client: (_run-qemu "service-client" "nuttx-rs-service-client")

# Run NuttX action server on QEMU
action-server: (_run-qemu "action-server" "nuttx-rs-action-server")

# Run NuttX action client on QEMU
action-client: (_run-qemu "action-client" "nuttx-rs-action-client")

# Start zenohd router for NuttX QEMU examples (port 7452)
# Start zenohd router for NuttX QEMU examples (port 7452)
# Must listen on 0.0.0.0 for virtio-net-device + slirp gateway (10.0.2.2)
zenohd:
    build/zenohd/zenohd --listen tcp/0.0.0.0:7452 --no-multicast-scouting

# Phase 127.B.5 — one-time host setup for NuttX DDS cross-QEMU mcast.
#
# QEMU's `-netdev socket,mcast=…,localaddr=127.0.0.1` puts the mcast
# egress on `lo`, but the kernel only loops frames back to a sibling
# QEMU when a route for the group is present on `lo`. Without these
# routes, two local NuttX QEMU instances silently fail to exchange
# RTPS SPDP frames and the DDS listener receives 0 messages.
#
# Idempotent — re-running drops + re-adds the routes. Requires sudo.
setup-mcast-route:
    #!/usr/bin/env bash
    set -e
    echo "Adding mcast loopback routes (requires sudo)..."
    sudo ip route del 230.10.0.0/16 dev lo 2>/dev/null || true
    sudo ip route del 239.0.0.0/8   dev lo 2>/dev/null || true
    sudo ip route add 230.10.0.0/16 dev lo
    sudo ip route add 239.0.0.0/8   dev lo
    echo "Routes added:"
    ip route show 230.10.0.0/16
    ip route show 239.0.0.0/8

# Clean NuttX build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-nuttx-armv7a-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-arm-nuttx/rust/$example/target"
    done
    echo "NuttX build artifacts cleaned"
