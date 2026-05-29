set working-directory := '..'

# cyclonedds module — build Cyclone DDS from the pinned submodule and
# (in later sub-phases) build the `nros-rmw-cyclonedds` C++ backend on
# top of it. See `docs/roadmap/phase-117-cyclonedds-rmw.md`.

default:
    @just --list --justfile {{source_file()}}

# Fetch the pinned Cyclone DDS submodule (the self-provision source).
#
# Phase 186.6.5 — there is no host Cyclone *build* any more: the backend and the
# examples self-provision Cyclone from this submodule via nros_provide_cyclonedds().
# `setup` just initialises the submodule and reports whether a host `idlc` (the
# Cyclone IDL compiler, used for descriptor codegen) is resolvable. Idempotent.
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -e
    # Phase 197.4 — provision the Cyclone DDS fork source via the SDK index
    # (cyclonedds is an RMW, not a board; its one source is [source.cyclonedds-src]).
    nros setup --source cyclonedds-src
    if command -v idlc >/dev/null 2>&1; then
        echo "Cyclone DDS submodule ready; idlc on PATH ($(command -v idlc))."
    else
        echo "Cyclone DDS submodule ready."
        echo "NOTE: no 'idlc' on PATH — Cyclone descriptor codegen needs a host"
        echo "      idlc (a ROS 2 / CycloneDDS install, or pass -DIDLC_EXECUTABLE=<path>)."
    fi

# Diagnose Cyclone DDS install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    missing=0
    # Phase 186.6.5 — Cyclone self-provisions from source; there is no host
    # `build/install`. Check the source submodule + a host idlc on PATH.
    if [ -f third-party/dds/cyclonedds/CMakeLists.txt ]; then
        echo "  [OK]      Cyclone source submodule:  third-party/dds/cyclonedds/"
    else
        echo "  [MISSING] Cyclone source submodule (run: just cyclonedds setup)"
        missing=1
    fi
    if command -v idlc >/dev/null 2>&1; then
        echo "  [OK]      idlc (host IDL compiler):  $(command -v idlc)"
    else
        echo "  [MISSING] idlc on PATH (install a ROS 2 / CycloneDDS, or set IDLC_EXECUTABLE)"
        missing=1
    fi
    # Phase 185.5 — embedded-RTOS cross-built `ddsc` installs. Provisioned on
    # demand by the platform `build-fixtures` (Phase 185.1) when the cross
    # toolchain is present (the `all` tier), NOT by `just cyclonedds setup`.
    # Report-only — absence is not a `missing` (the embedded-Cyclone tests are
    # filtered out of a tier that can't build them, Phase 185.2).
    echo "  --- embedded Cyclone ---"
    # Phase 186 — freertos Cyclone is self-provisioned per-example by CMake
    # (nros_provide_cyclonedds(), built inside each build-cyclonedds/ tree from
    # the pinned submodule); there is no shared install artifact to check. Needs
    # only arm-none-eabi-gcc on PATH.
    if command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "  [OK]      freertos: arm-none-eabi-gcc present → Cyclone self-provisions in CMake (Phase 186)"
    else
        echo "  [ABSENT]  freertos: arm-none-eabi-gcc not found → embedded Cyclone tests skip (install the ARM toolchain)"
    fi
    # Phase 186 — threadx-rv64 Cyclone is also self-provisioned per-example by
    # CMake (no shared install). Needs only the RV64 toolchain; the embedded
    # fixtures stay behind the experimental NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1.
    if command -v riscv64-unknown-elf-gcc >/dev/null 2>&1; then
        echo "  [OK]      threadx-rv64: riscv64-unknown-elf-gcc present → Cyclone self-provisions in CMake (Phase 186; experimental, set NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1)"
    else
        echo "  [ABSENT]  threadx-rv64: riscv64-unknown-elf-gcc not found → embedded Cyclone tests skip (install the RV64 toolchain)"
    fi
    exit $missing

# Phase 175.B — read-only inventory of the Cyclone DDS ddsrt RTOS port
# surface. This does not claim RTOS Cyclone support is enabled; it
# distinguishes the existing upstream FreeRTOS+lwIP ddsrt pieces from the
# missing ThreadX/NetX Duo port.
[group("debug")]
ddsrt-port-inventory:
    bash ./scripts/cyclonedds/ddsrt-port-inventory.sh

# Build the nros-rmw-cyclonedds backend (self-provisions Cyclone from source).
# Phase 186.6.5 — alias for `build-rmw`; there is no separate host-Cyclone build.
[group("main")]
build: build-rmw

# Clean Cyclone build artifacts (does not touch the submodule). Phase 186.6.5 —
# removes the backend build tree + any legacy host install / build dirs.
[group("main")]
clean:
    rm -rf packages/dds/nros-rmw-cyclonedds/build build/install build/cyclonedds

# Build the `nros-rmw-cyclonedds` C++ backend on top of Cyclone DDS.
# Standalone CMake project at packages/dds/nros-rmw-cyclonedds/.
# Phase 186.6.1 — self-provisions Cyclone from the pinned source
# (nros_provide_cyclonedds()), so no `just cyclonedds setup` / `build/install`
# prereq. Host flags mirror scripts/cyclonedds/build.sh (ENABLE_*/SHM off) +
# a static ddsc; idlc resolves from the source build / PATH (186.3). No install
# step — the ctest harness runs from the build tree.
[group("main")]
build-rmw:
    #!/usr/bin/env bash
    set -e
    PROJ="packages/dds/nros-rmw-cyclonedds"
    SCRIPT="$PWD/scripts/cyclonedds/msg_to_cyclone_idl.py"
    # Pass the .msg → IDL converter as an absolute path so the
    # project never has to assume the layout above its own dir
    # (CLAUDE.md "CMake Path Convention").
    cmake -S "$PROJ" -B "$PROJ/build" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON \
        -DCYCLONEDDS_SOURCE_DIR="$PWD/third-party/dds/cyclonedds" \
        -DENABLE_SECURITY=OFF -DENABLE_SSL=OFF -DENABLE_SHM=OFF \
        -DBUILD_SHARED_LIBS=OFF \
        -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL="$SCRIPT" \
        -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL_SOURCE="$SCRIPT"
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        cmake --build "$PROJ/build"
    else
        cmake --build "$PROJ/build" --parallel
    fi

# Run Cyclone DDS RMW tests via CTest.
# Phase 117.3 ships only the register-vtable smoke test; 117.12 adds
# the POSIX E2E pub/sub interop harness against stock rmw_cyclonedds.
[group("main")]
test: build-rmw
    ctest --test-dir packages/dds/nros-rmw-cyclonedds/build --output-on-failure

# Composite: build RMW backend (self-provisions Cyclone) + run tests. Wired
# into top-level `just ci` via the `cyclonedds-ci` wrapper as of Phase 117.16.
# Phase 186.6.1 — no `setup` prereq; build-rmw self-provisions from source.
[group("full-matrix")]
ci: build-rmw test
