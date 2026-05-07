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
setup:
    #!/usr/bin/env bash
    set -e
    if [ ! -f third-party/dds/cyclonedds/CMakeLists.txt ]; then
        echo "Initialising Cyclone DDS submodule..."
        git submodule update --init --recursive third-party/dds/cyclonedds
    fi
    if [ -f build/install/lib/libddsc.so ] && [ -x build/install/bin/idlc ]; then
        echo "Cyclone DDS already built: build/install/lib/libddsc.so"
        echo "To rebuild: just cyclonedds clean && just cyclonedds setup"
    else
        ./scripts/cyclonedds/build.sh
    fi

# Diagnose Cyclone DDS install status (read-only).
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
    exit $missing

# Build Cyclone DDS (alias for `just cyclonedds setup`).
build:
    ./scripts/cyclonedds/build.sh

# Clean Cyclone DDS build artifacts (does not touch the submodule).
clean:
    ./scripts/cyclonedds/build.sh --clean

# Build the `nros-rmw-cyclonedds` C++ backend on top of Cyclone DDS.
# Standalone CMake project at packages/dds/nros-rmw-cyclonedds/.
# Configures + builds + installs into the shared `build/install/`
# prefix so downstream nros-cpp examples can find it via
# `find_package(NrosRmwCyclonedds)`.
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
    cmake --build "$PROJ/build" --parallel
    cmake --install "$PROJ/build"

# Run Cyclone DDS RMW tests via CTest.
# Phase 117.3 ships only the register-vtable smoke test; 117.12 adds
# the POSIX E2E pub/sub interop harness against stock rmw_cyclonedds.
test: build-rmw
    ctest --test-dir packages/dds/nros-rmw-cyclonedds/build --output-on-failure

# Composite: setup Cyclone + build RMW backend + run tests. Wired
# into top-level `just ci` via the `cyclonedds-ci` wrapper as of
# Phase 117.16.
ci: setup build-rmw test
