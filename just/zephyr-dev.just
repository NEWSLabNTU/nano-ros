build:
    @echo "Zephyr has no separate core build; use 'just zephyr build-examples' for the matrix, or 'just zephyr build-one <example> <rmw>' for one."

# Build ONE Zephyr example for the selected line (NROS_ZEPHYR_VERSION).
# Reproduces the Phase 180.A orchestration: host nros-codegen + (4.4) the
# Python 3.12 venv on PATH + host toolchain. Assumes `just zephyr setup`
# already ran (it applies the 3.7 patch set / provisions the 4.4 venv).
#   just zephyr build-one c/talker zenoh
#   NROS_ZEPHYR_VERSION=4.4 just zephyr build-one c/talker zenoh
[group("examples")]
build-one example rmw="zenoh" board="native_sim/native/64":
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/build/cargo.sh
    # Phase 180.B — examples read the ROS interface package dirs from env
    # (no more hardcoded /opt/ros/humble/share/* PKG_DIR). Default to the
    # stock Humble install; override per package if your ROS lives elsewhere.
    export NROS_STD_MSGS_DIR="${NROS_STD_MSGS_DIR:-/opt/ros/humble/share/std_msgs}"
    export NROS_EXAMPLE_INTERFACES_DIR="${NROS_EXAMPLE_INTERFACES_DIR:-/opt/ros/humble/share/example_interfaces}"
    export NROS_BUILTIN_INTERFACES_DIR="${NROS_BUILTIN_INTERFACES_DIR:-/opt/ros/humble/share/builtin_interfaces}"
    export NROS_UNIQUE_IDENTIFIER_MSGS_DIR="${NROS_UNIQUE_IDENTIFIER_MSGS_DIR:-/opt/ros/humble/share/unique_identifier_msgs}"
    export NROS_ACTION_MSGS_DIR="${NROS_ACTION_MSGS_DIR:-/opt/ros/humble/share/action_msgs}"
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr {{NROS_ZEPHYR_VERSION}} workspace not set up — run:"
        echo "  NROS_ZEPHYR_VERSION={{NROS_ZEPHYR_VERSION}} just zephyr setup"
        exit 1
    fi
    # 4.4 line: run west through the Python 3.12 venv (Task 2b) + host
    # toolchain (native_sim = host gcc). Invoke west via the venv interpreter
    # EXPLICITLY rather than prepending the venv to PATH — a PATH prepend also
    # shadows `python3` for build subprocesses, which breaks the cyclonedds
    # descriptor codegen (ROS msg2idl needs catkin_pkg/rosidl_adapter from the
    # system ROS python 3.10, not the venv 3.12). west passes WEST_PYTHON to
    # cmake so find_package(Python3>=3.12) is still satisfied.
    venvbin="{{ZEPHYR_VENV_BIN}}"
    west_cmd=(west)
    if [ -n "$venvbin" ]; then
        west_cmd=("$(realpath "$venvbin")/python" -m west)
    fi
    # issue #87 — host gcc toolchain for native_sim on ANY line (board-keyed,
    # not version-keyed). Real boards leave the variant unset → SDK. The venv
    # python switch above stays 4.4-specific (find_package Python3>=3.12).
    case "{{board}}" in
    native_sim*) export ZEPHYR_TOOLCHAIN_VARIANT="${ZEPHYR_TOOLCHAIN_VARIANT:-host}" ;;
    esac
    # Host codegen tool — nros_generate_interfaces() needs it (version-independent).
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
    src="$(pwd)/examples/zephyr/{{example}}"
    [ -d "$src" ] || { echo "no example at $src"; exit 1; }
    # Rust examples consume the generated interface crates (std_msgs +
    # transitively builtin_interfaces, …) as cargo path-deps via
    # .cargo/config.toml's [patch.crates-io] — those crates must exist before the
    # cargo-driven west build. The C/C++ paths codegen in cmake via
    # nros_generate_interfaces(); the Rust path has no cmake equivalent, so run
    # `nros generate-rust` here. Needs ROS sourced (AMENT_PREFIX_PATH resolves
    # the .msg defs — CI sources it; locally `source /opt/ros/<distro>/setup.bash`).
    case "{{example}}" in
    rust/*)
        echo "Zephyr: generating Rust interface crates for {{example}}"
        # Phase 214.I.2 — fail-loud prereq guard: `ws sync` is absent
        # from the shipped 0.3.7 release. Probe; emit `[PREREQ]` + exit 0
        # if missing rather than cascading into clap noise.
        nros_require_ws_sync "$codegen_tool"
        # Phase 214.J.2 — wipe `generated/` on trait-surface drift before sync.
        source "$(pwd)/scripts/build/codegen-stamp.sh"
        NROS_REPO_DIR="$(pwd)" nros_codegen_stamp_check_or_wipe "$src"
        NROS_REPO_DIR="$(pwd)" "$codegen_tool" sync "$src"
        NROS_REPO_DIR="$(pwd)" nros_codegen_stamp_write "$src"
        ;;
    esac
    # CONF_FILE = base + RMW overlay + (native_sim) the version-aware NSOS
    # line overlay. The line overlay (cmake/zephyr/native-sim-line-<v>.conf)
    # supplies the native-eth-driver disable under the right symbol for the
    # line (ETH_NATIVE_POSIX on 3.7, ETH_NATIVE_TAP on 4.4) plus the shared
    # NSOS settings — so NSOS works on BOTH lines. It supersedes the legacy
    # per-example boards/native_sim_*.conf (which hardcodes the 3.7 symbol);
    # that file is left for the not-yet-version-gated build-fixtures path.
    conf="prj.conf;prj-{{rmw}}.conf"
    case "{{board}}" in
    native_sim*)
        line_overlay="$(pwd)/cmake/zephyr/native-sim-line-{{NROS_ZEPHYR_VERSION}}.conf"
        if [ -f "$line_overlay" ]; then
            conf="$conf;$line_overlay"
        else
            echo "WARN: no native_sim line overlay at $line_overlay" >&2
        fi
        ;;
    esac
    bd="build-$(echo '{{example}}' | tr '/' '-')-{{rmw}}"
    echo "Zephyr {{NROS_ZEPHYR_VERSION}}: building zephyr/{{example}} ({{rmw}}, {{board}}) -> $workspace/$bd"
    cd "$workspace"
    "${west_cmd[@]}" build -b "{{board}}" -d "$bd" "$src" -- \
        -DCONF_FILE="$conf" -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool"
    echo "Built: $workspace/$bd/zephyr/zephyr.elf"

# Phase 180.B item 5 — copy-out CI check. Copies an example OUT of the
# repo tree and builds it against the nano-ros Zephyr module on the 4.4
# line, proving the module-only consumption contract (no repo-tree
# walk-up). PASS iff it reaches zephyr.elf. Idempotent; cleans its temp
# dir. Args: <example> <rmw> <board> (defaults c/talker zenoh native_sim).
#   just zephyr check-copy-out
#   just zephyr check-copy-out c/listener zenoh
[group("examples")]
check-copy-out example="c/talker" rmw="zenoh" board="native_sim/native/64":
    ./scripts/zephyr/check-copy-out.sh "{{example}}" "{{rmw}}" "{{board}}"

# Build Zephyr Rust examples (all Rust examples for native_sim)
[group("examples")]
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
    # Delegate each role to the maintained single-example builder
    # (`build-one` owns the west-invocation + workspace setup; run from
    # the repo root, so do NOT cd into the workspace here).
    for ex in talker listener service-server service-client action-server action-client; do
        just zephyr build-one "rust/$ex" zenoh
    done
    echo "Zephyr Rust examples built successfully!"

# Build Zephyr C examples
[group("examples")]
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
[group("examples")]
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
    CODEGEN="$(nros_cargo_codegen_c_bin)"
    MAKE_BIN="$(pwd)/third-party/make/make"
    if [ ! -x "$MAKE_BIN" ]; then
        MAKE_BIN="$(command -v make)"
    fi
    export PATH="$(pwd)/third-party/make:$(pwd)/third-party/ninja:$PATH"
    if [ ! -x "$CODEGEN" ]; then
        echo "  Building host nros-codegen..."
        cargo_profile_args="$(nros_cargo_profile_arg_string)"
        nros_cargo_ensure_codegen_c
    fi
    # Phase 220.D — resolve absolute paths up front. The default workspace
    # path is relative (`zephyr-workspace` or `../nano-ros-workspace`),
    # which both `west build -d` and the fixture source-dir argument resolve
    # against cwd; mixing that with the in-workspace `cd` made the recipe
    # fragile. Pin both to absolute paths and pass the fixture source as an
    # absolute path so west's pristine cache + SOURCE_DIR stay in sync.
    NROS_ROOT="$(pwd)"
    WORKSPACE_ABS="$(cd "$WORKSPACE" && pwd)"
    FIXTURE_SRC="$NROS_ROOT/packages/testing/nros-tests/bins/logging-smoke-zephyr-native-sim"
    BUILD_DIR="$WORKSPACE_ABS/build-logging-smoke"
    # Phase 220.D — guard against a stale CMakeCache from a previous
    # workspace layout (e.g. legacy in-tree `nano-ros/zephyr-workspace/`).
    # `-p auto` defers to the cache's `pristine.cmake`; if that path no
    # longer exists, the build fails before any cmake re-configure.
    CACHE_FILE="$BUILD_DIR/CMakeCache.txt"
    if [ -f "$CACHE_FILE" ]; then
        cached_zephyr_base="$(sed -n 's/^ZEPHYR_BASE:PATH=//p' "$CACHE_FILE" | head -1)"
        if [ -n "$cached_zephyr_base" ] && [ ! -d "$cached_zephyr_base" ]; then
            echo "  Stale CMakeCache (ZEPHYR_BASE=$cached_zephyr_base no longer exists) — wiping $BUILD_DIR"
            rm -rf "$BUILD_DIR"
        fi
    fi
    echo "  Building logging-smoke-zephyr-native-sim -> $BUILD_DIR"
    # issue 0093 — Zephyr 4.4's `find_package(Python3)` requires >=3.12. Prepend the
    # provisioned py3.12 venv to PATH (empty on the 3.7 line) so `west`/cmake resolve
    # the 3.12 python — same as the example-fixture path (zephyr-ci.just). Without it
    # this testing-bin fixture configures with the system python (3.10) and 4.4 fails.
    venvbin="{{ZEPHYR_VENV_BIN}}"
    if [ -n "$venvbin" ]; then
        export PATH="$(realpath "$venvbin"):$PATH"
    fi
    # issue #87 — native_sim uses host gcc (no Zephyr SDK download). Board is
    # always native_sim here; export host unless the caller overrode it.
    export ZEPHYR_TOOLCHAIN_VARIANT="${ZEPHYR_TOOLCHAIN_VARIANT:-host}"
    cd "$WORKSPACE_ABS"
    west build -b native_sim/native/64 -d "$BUILD_DIR" -p auto \
        "$FIXTURE_SRC" \
        -- -D_NANO_ROS_CODEGEN_TOOL="$CODEGEN" -DMAKE="$MAKE_BIN"

# Build Zephyr C++ examples
[group("examples")]
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
[group("examples")]
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
[group("main")]
build-examples: build-rust-examples build-c build-cpp build-xrce
    @echo "All Zephyr examples built!"

# Build every Zephyr tier: core artifacts, examples, and test fixtures.
[group("full-matrix")]
build-all: build build-examples build-fixtures
    @echo "Zephyr build-all completed."

# Run Zephyr E2E tests (requires pre-built Zephyr examples; native_sim uses NSOS on host loopback)
#
# Phase 214.Q: `build-fixtures` is sequenced as a prereq so the nextest
# harness never explodes with "Zephyr fixture binary is stale" — recipes
# own the build-then-test sequencing.
[group("main")]
test verbose="": build-fixtures
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    args=(-p nros-tests --test zephyr --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run all Zephyr tests (zenoh + XRCE + C)
[group("full-matrix")]
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
[group("examples")]
build-c-port:
    #!/usr/bin/env bash
    set -e
    # Phase 220.D — derive workspace from the same {{ZEPHYR_WORKSPACE}}
    # used by every other zephyr recipe; the old hardcoded
    # `zephyr-workspace/env.sh` only worked under the legacy in-tree
    # layout and silently broke under the canonical
    # `../nano-ros-workspace` sibling layout produced by Phase 218 setup.
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -f "$WORKSPACE/env.sh" ]; then
        echo "$WORKSPACE/env.sh missing — run: just zephyr setup" >&2
        exit 2
    fi
    make_bin="$(pwd)/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    export PATH="$(pwd)/third-party/make:$(pwd)/third-party/ninja:$PATH"
    source "$WORKSPACE/env.sh"
    west build -b native_sim/native/64 \
        -d tests/zephyr-c-smoke/build tests/zephyr-c-smoke \
        -- -DMAKE="$make_bin"

# Phase 121.6 — run the Zephyr native_sim smoke test on the host.
[group("debug")]
test-c-port: build-c-port
    #!/usr/bin/env bash
    set -e
    timeout 15 ./tests/zephyr-c-smoke/build/zephyr/zephyr.exe

# Phase 180.A Task 10 — dual-line CI. Build a representative example
# (default c/talker zenoh) on BOTH the 3.7 LTS line and the 4.4 rolling
# line via `build-one`, reporting per-line pass/fail. A line whose
# workspace is not set up SKIPS with a message (never a silent pass);
# a line whose workspace IS set up but fails to build is a hard FAIL.
# Exit non-zero iff any present line failed.
#   just zephyr ci-both
#   just zephyr ci-both cpp/listener zenoh
[group("full-matrix")]
ci-both example="c/talker" rmw="zenoh":
    #!/usr/bin/env bash
    set -uo pipefail
    declare -A WS=( [3.7]="${NROS_ZEPHYR_WORKSPACE_37:-../nano-ros-workspace}"
                    [4.4]="${NROS_ZEPHYR_WORKSPACE_44:-../nano-ros-workspace-4.4}" )
    overall=0
    declare -A RESULT
    for line in 3.7 4.4; do
        ws="${WS[$line]}"
        ws_abs="$(realpath "$ws" 2>/dev/null || echo "")"
        if [ -z "$ws_abs" ] || [ ! -d "$ws_abs/zephyr" ]; then
            echo "=== Zephyr $line: SKIP (workspace not set up at $ws — run: NROS_ZEPHYR_VERSION=$line just zephyr setup)"
            RESULT[$line]="SKIP"
            continue
        fi
        echo "=== Zephyr $line: building zephyr/{{example}} ({{rmw}}) via build-one ==="
        if NROS_ZEPHYR_VERSION="$line" NROS_ZEPHYR_WORKSPACE="$ws_abs" \
            just zephyr build-one "{{example}}" "{{rmw}}"; then
            echo "=== Zephyr $line: PASS ==="
            RESULT[$line]="PASS"
        else
            echo "=== Zephyr $line: FAIL ==="
            RESULT[$line]="FAIL"
            overall=1
        fi
    done
    echo
    echo "================ dual-line summary ================"
    for line in 3.7 4.4; do
        printf "  Zephyr %-4s : %s\n" "$line" "${RESULT[$line]}"
    done
    echo "==================================================="
    if [ "$overall" -ne 0 ]; then
        echo "FAIL: at least one present Zephyr line failed."
        exit 1
    fi
    echo "OK: all present Zephyr lines built (skips reported above)."

# CI: doctor + test
[group("full-matrix")]
ci: doctor test

# Run Zephyr tests with full rebuild
[group("debug")]
test-full verbose="": build-examples
    just zephyr test {{verbose}}

# Run Zephyr XRCE E2E tests
[group("debug")]
test-xrce verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=(-p nros-tests --test zephyr --no-fail-fast -E 'test(xrce)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run Zephyr C examples test
[group("debug")]
test-c:
    just build-zenohd
    ./tests/zephyr/run-c.sh

# Clean Zephyr build directories
[group("main")]
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

# Start zenohd router for Zephyr examples (allocator rust-pubsub port 7400, all interfaces)
zenohd:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/zenohd.sh
    exec "$(nros_zenohd_bin)" --listen tcp/0.0.0.0:7400 --no-multicast-scouting
