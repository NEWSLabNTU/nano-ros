set working-directory := '..'

# cyclonedds module — build Cyclone DDS from the pinned submodule and
# (in later sub-phases) build the `nros-rmw-cyclonedds` C++ backend on
# top of it. See `docs/roadmap/phase-117-cyclonedds-rmw.md`.

default:
    @just --list --justfile {{source_file()}}

# Build Cyclone DDS from the pinned submodule (skipped if already
# built). Installs into the shared `build/install/` prefix so
# `find_package(CycloneDDS)` finds it via `CMAKE_PREFIX_PATH`.
#
# Initialises the submodule on first run; idempotent on re-runs.
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -e
    # Phase 123.A.8 — submodule fetch via tools/setup.sh.
    "{{justfile_directory()}}/tools/setup.sh" --rmw=cyclonedds --skip-rustup --skip-apt-check
    if [ -f build/install/lib/libddsc.so ] && [ -x build/install/bin/idlc ]; then
        echo "Cyclone DDS already built: build/install/lib/libddsc.so"
        echo "To rebuild: just cyclonedds clean && just cyclonedds setup"
    else
        ./scripts/cyclonedds/build.sh
    fi

# Diagnose Cyclone DDS install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    missing=0
    if [ -f build/install/lib/libddsc.so ]; then
        echo "  [OK]      libddsc.so:                build/install/lib/libddsc.so"
    else
        echo "  [MISSING] libddsc.so (run: just cyclonedds setup)"
        missing=1
    fi
    if [ -x build/install/bin/idlc ]; then
        echo "  [OK]      idlc:                      build/install/bin/idlc"
    else
        echo "  [MISSING] idlc (run: just cyclonedds setup)"
        missing=1
    fi
    if [ -f build/install/lib/cmake/CycloneDDS/CycloneDDSConfig.cmake ]; then
        echo "  [OK]      CycloneDDS cmake config:   build/install/lib/cmake/CycloneDDS/"
    else
        echo "  [MISSING] CycloneDDS cmake config (run: just cyclonedds setup)"
        missing=1
    fi
    # Phase 185.5 — embedded-RTOS cross-built `ddsc` installs. Provisioned on
    # demand by the platform `build-fixtures` (Phase 185.1) when the cross
    # toolchain is present (the `all` tier), NOT by `just cyclonedds setup`.
    # Report-only — absence is not a `missing` (the embedded-Cyclone tests are
    # filtered out of a tier that can't build them, Phase 185.2).
    echo "  --- embedded cross ddsc (provisioned by <plat> build-fixtures, 'all' tier) ---"
    if [ -f build/cyclonedds-freertos-install/lib/libddsc.a ]; then
        echo "  [OK]      freertos cross ddsc:       build/cyclonedds-freertos-install/lib/libddsc.a"
    else
        echo "  [ABSENT]  freertos cross ddsc (auto-built by 'just freertos build-fixtures' w/ arm-none-eabi-gcc; or 'just cyclonedds freertos-cross-probe')"
    fi
    if [ -f build/cyclonedds-threadx-rv64-install/lib/libddsc.a ]; then
        echo "  [OK]      threadx-rv64 cross ddsc:   build/cyclonedds-threadx-rv64-install/lib/libddsc.a"
    else
        echo "  [ABSENT]  threadx-rv64 cross ddsc (NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1 just threadx_riscv64 build-fixtures; or 'just cyclonedds threadx-cross-probe')"
    fi
    exit $missing

# Phase 175.B — read-only inventory of the Cyclone DDS ddsrt RTOS port
# surface. This does not claim RTOS Cyclone support is enabled; it
# distinguishes the existing upstream FreeRTOS+lwIP ddsrt pieces from the
# missing ThreadX/NetX Duo port.
[group("debug")]
ddsrt-port-inventory:
    bash ./scripts/cyclonedds/ddsrt-port-inventory.sh

# Phase 175.B — configure/build probe for Cyclone DDS on the nano-ros
# FreeRTOS MPS2 + lwIP target. This is diagnostic only; it does not enable
# Cyclone fixture cells until the recorded port/config blockers are fixed.
[group("debug")]
freertos-cross-probe:
    bash ./scripts/cyclonedds/freertos-cross-probe.sh

# Phase 175.B — configure/build probe for Cyclone DDS on the nano-ros
# ThreadX RISC-V64 + NetX Duo target. This validates the experimental
# nano-ros ThreadX ddsrt port before fixture cells are enabled.
[group("debug")]
threadx-cross-probe:
    bash ./scripts/cyclonedds/threadx-cross-probe.sh

# Build Cyclone DDS (alias for `just cyclonedds setup`).
[group("main")]
build:
    ./scripts/cyclonedds/build.sh

# Clean Cyclone DDS build artifacts (does not touch the submodule).
[group("main")]
clean:
    ./scripts/cyclonedds/build.sh --clean

# Build the `nros-rmw-cyclonedds` C++ backend on top of Cyclone DDS.
# Standalone CMake project at packages/dds/nros-rmw-cyclonedds/.
# Configures + builds + installs into the shared `build/install/`
# prefix so downstream nros-cpp examples can find it via
# `find_package(NrosRmwCyclonedds)`.
[group("main")]
build-rmw: setup
    #!/usr/bin/env bash
    set -e
    PROJ="packages/dds/nros-rmw-cyclonedds"
    SCRIPT="$PWD/scripts/cyclonedds/msg_to_cyclone_idl.py"
    # Pass the .msg → IDL converter as an absolute path so the
    # project never has to assume the layout above its own dir
    # (CLAUDE.md "CMake Path Convention").
    cmake -S "$PROJ" -B "$PROJ/build" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_PREFIX_PATH="$PWD/build/install" \
        -DCMAKE_INSTALL_PREFIX="$PWD/build/install" \
        -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL="$SCRIPT" \
        -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL_SOURCE="$SCRIPT"
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        cmake --build "$PROJ/build"
    else
        cmake --build "$PROJ/build" --parallel
    fi
    cmake --install "$PROJ/build"

# Run Cyclone DDS RMW tests via CTest.
# Phase 117.3 ships only the register-vtable smoke test; 117.12 adds
# the POSIX E2E pub/sub interop harness against stock rmw_cyclonedds.
[group("main")]
test: build-rmw
    ctest --test-dir packages/dds/nros-rmw-cyclonedds/build --output-on-failure

# Composite: setup Cyclone + build RMW backend + run tests. Wired
# into top-level `just ci` via the `cyclonedds-ci` wrapper as of
# Phase 117.16.
[group("full-matrix")]
ci: setup build-rmw test
