set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}
# Zephyr line selector (Phase 180.A). "3.7" = LTS (default, west.yml);
# "4.4" = latest rolling (west-4.4.yml, sibling workspace). Set
# $NROS_ZEPHYR_VERSION to pick. Support policy + add-a-line checklist:
# docs/development/zephyr-version-support.md (floor is 3.7 — the
# zephyr-lang-rust × zephyr pair; nothing below ships the Rust module).
NROS_ZEPHYR_VERSION := env("NROS_ZEPHYR_VERSION", "3.7")

# West manifest for the selected line.
ZEPHYR_MANIFEST := if NROS_ZEPHYR_VERSION == "4.4" { "west-4.4.yml" } else { "west.yml" }

# Zephyr workspace path. Default: in-tree (gitignored) `zephyr-workspace/`.
# Falls back to legacy sibling `../nano-ros-workspace` for users set up
# before the in-tree default. The 4.4 line uses its own sibling so the
# two trees coexist. Set $NROS_ZEPHYR_WORKSPACE to override either.
ZEPHYR_WORKSPACE := env("NROS_ZEPHYR_WORKSPACE", if NROS_ZEPHYR_VERSION == "4.4" { "../nano-ros-workspace-4.4" } else if path_exists("zephyr-workspace") == "true" { "zephyr-workspace" } else if path_exists("../nano-ros-workspace") == "true" { "../nano-ros-workspace" } else { "zephyr-workspace" })

# Python 3.12 venv bin for the 4.4 line (Task 2b). 4.4's
# find_package(Python3) requires >=3.12; the venv provisions it without
# sudo. Recipes that invoke `west` on the 4.4 line prepend this to PATH
# so bare `west` resolves to the venv's (3.12) one. Empty on 3.7.
ZEPHYR_VENV_BIN := if NROS_ZEPHYR_VERSION == "4.4" { ZEPHYR_WORKSPACE / ".venv312/bin" } else { "" }

# Install the Zephyr workspace at $repo/zephyr-workspace/ (large download).
# Idempotent: skips if the workspace already exists.
# Set $NROS_ZEPHYR_WORKSPACE to install elsewhere.
# Extra args are forwarded: e.g. `just zephyr setup --force --skip-sdk`.
[group("setup")]
setup *ARGS:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    ARGS="{{ARGS}}"
    # Phase 197.1 — provision the in-tree sources the Zephyr build needs the way a
    # user/CI does (`nros setup --source`, index-driven), NOT a hand
    # `git submodule update`: zenoh-pico (zenoh examples), the cyclonedds fork (the
    # patches below operate on the in-tree submodule), and px4-rs (the root
    # workspace's cargo load path-deps px4-sitl-tests). Mirrors the CI workflow so
    # local == CI. nros resolved from $NROS_CLI / PATH / ~/.nros (install.sh).
    source scripts/build/cargo.sh
    nros_bin="$(nros_cli_bin)"
    echo "[zephyr setup] provisioning sources via nros ($nros_bin)..."
    "$nros_bin" setup --source zenoh-pico --source cyclonedds-src --source px4-rs \
        --index "$(pwd)/nros-sdk-index.toml"
    if [ -d "$WORKSPACE/zephyr" ] && [[ "$ARGS" != *--force* ]]; then
        echo "Zephyr workspace already present at $WORKSPACE"
        echo "To reinstall, run: just zephyr setup --force"
    else
        NROS_ZEPHYR_MANIFEST="{{ZEPHYR_MANIFEST}}" NROS_ZEPHYR_WORKSPACE="$WORKSPACE" ./scripts/zephyr/setup.sh $ARGS
    fi
    # Phase 160.B — ensure the Cortex-A9 (qemu_cortex_a9) toolchain
    #   1. `armv7a-none-eabi` rust target (no_std core / compiler_builtins)
    #   2. `cortex-a9-rust-patch.sh`: patches Zephyr's Rust module to
    #      register the Cortex-A9 target + adds the SLCR MMU entry the
    #      Zynq-7000 eth_xlnx_gem driver needs.
    # Both are idempotent — safe to run on every `just zephyr setup`.
    echo "[zephyr setup] ensuring armv7a-none-eabi Rust target..."
    rustup target add armv7a-none-eabi
    # Phase 199.3 — version-dispatched patch set. The per-line patch sequences
    # (formerly an inline `if NROS_ZEPHYR_VERSION = 4.4 … else …` here) live in
    # scripts/zephyr/patches/<version>.sh. Adding a Zephyr line = drop a new
    # sibling there; no edit to this recipe.
    PATCHSET="./scripts/zephyr/patches/{{NROS_ZEPHYR_VERSION}}.sh"
    if [ ! -f "$PATCHSET" ]; then
        echo "[zephyr setup] ERROR: no patch set for Zephyr {{NROS_ZEPHYR_VERSION}} ($PATCHSET)." >&2
        echo "  Supported lines: $(ls ./scripts/zephyr/patches/*.sh | xargs -n1 basename | sed 's/\.sh$//' | tr '\n' ' ')" >&2
        echo "  Add scripts/zephyr/patches/{{NROS_ZEPHYR_VERSION}}.sh (see patches/README.md)." >&2
        exit 1
    fi
    echo "[zephyr setup] applying {{NROS_ZEPHYR_VERSION}} patch set ($PATCHSET)..."
    bash "$PATCHSET" "$WORKSPACE"
    # The Micro-XRCE-DDS Agent is only needed for XRCE *runtime* tests. Builds
    # that use a different RMW (e.g. the zenoh dual-line CI) can skip the heavy
    # Fast-DDS superbuild via NROS_ZEPHYR_SKIP_XRCE_AGENT.
    if [ -n "${NROS_ZEPHYR_SKIP_XRCE_AGENT:-}" ]; then
        echo "[zephyr setup] skipping Micro XRCE-DDS Agent (NROS_ZEPHYR_SKIP_XRCE_AGENT set)"
    else
        echo "[zephyr setup] ensuring Micro XRCE-DDS Agent for Zephyr XRCE tests..."
        just xrce setup
    fi

# Diagnose Zephyr install status (read-only).
[group("setup")]
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
    if just xrce doctor; then
        :
    else
        fail=1
    fi
    # native_sim uses NSOS (host loopback) — no TAP bridge required.
    exit $fail

# Build Zephyr core artifacts.
[group("main")]
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
        export ZEPHYR_TOOLCHAIN_VARIANT=host
        west_cmd=("$(realpath "$venvbin")/python" -m west)
    fi
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
        NROS_REPO_DIR="$(pwd)" "$codegen_tool" ws sync "$src"
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
    source scripts/build/fixture-matrix.sh
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
    echo "  → zephyr Rust generated-dir preflight"
    nros_cli="$(nros_cli_bin)"
    while IFS= read -r pkg; do
        dir="$(dirname "$pkg")"
        stamp="$dir/build/.nros-ws-sync.stamp"
        if [ "${NROS_ZEPHYR_GENERATE_RUST_FORCE:-0}" = "1" ] || [ ! -f "$stamp" ] || [ "$pkg" -nt "$stamp" ]; then
            echo "    ws sync: $dir"
            mkdir -p "$dir/build"
            NROS_REPO_DIR="$(pwd)" "$nros_cli" ws sync "$dir"
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
    entries=()
    # Phase 180.A — version-aware NSOS overlay. 4.4 renamed the native eth
    # driver disable (ETH_NATIVE_POSIX -> ETH_NATIVE_TAP), so the 4.4 line
    # uses the per-line overlay; 3.7 keeps the legacy conf.
    if [ "{{NROS_ZEPHYR_VERSION}}" = "4.4" ]; then
        native_sim_nsos_conf="$nros_root/cmake/zephyr/native-sim-line-4.4.conf"
    else
        native_sim_nsos_conf="$nros_root/zephyr/native-sim-nsos.conf"
    fi
    # Phase 178.E guardrail: keep Zephyr E2E coverage on the real
    # user-copyable examples. Do not collapse roles into one runtime
    # dispatch image here; each role remains its own Zephyr app/image so
    # tests prove `examples/zephyr/<lang>/<role>` builds standalone.
    # RMW still varies by overlay, matching the user build shape.
    if [ "{{NROS_ZEPHYR_VERSION}}" = "4.4" ]; then
        # Phase 180.A — only the zenoh native_sim line is proven on 4.4.
        # xrce + cyclonedds on 4.4 await Tasks 5-6 / the cyclonedds-on-4.4
        # bring-up; restrict the 4.4 matrix to zenoh until they land.
        fixture_rmws=(zenoh)
        echo "  → 4.4 line: zenoh fixtures only (xrce/cyclonedds pending)"
    else
    fixture_rmws=(zenoh xrce)
    # Phase 186.6.2 — gate on idlc being *available* (host PATH or the legacy
    # build/install), not specifically build/install. The Zephyr Cyclone build
    # self-provisions Cyclone in-tree and resolves idlc host-side via find_program
    # (186.3 — NO_CMAKE_FIND_ROOT_PATH, e.g. a ROS 2 install on PATH), so no
    # `just cyclonedds setup` is needed.
    nros_store_root="${NROS_HOME:-$HOME/.nros}/sdk"
    if command -v idlc >/dev/null 2>&1 \
        || [ -n "$(ls "$nros_store_root"/cyclonedds/*/bin/idlc 2>/dev/null)" ] \
        || [ -x "$nros_root/build/cyclonedds/bin/idlc" ] \
        || [ -x "$nros_root/build/install/bin/idlc" ]; then
        fixture_rmws+=(cyclonedds)
    else
        echo "  → zephyr cyclonedds fixtures skipped (idlc not found on PATH or nros store — run \`nros setup <board> --rmw cyclonedds\`, install a ROS 2 idlc, or set IDLC_EXECUTABLE)"
    fi
    fi
    for lang in $(nros_fixture_langs); do
        lang_tag="$(nros_zephyr_lang_tag "$lang")"
        for rmw in "${fixture_rmws[@]}"; do
            for case in $(nros_fixture_roles); do
                xrce_port=""
                if [ "$rmw" = "xrce" ]; then
                    case "$lang" in
                        rust) lang_offset=0 ;;
                        c) lang_offset=100 ;;
                        cpp) lang_offset=200 ;;
                        *)
                            echo "Unknown Zephyr fixture language: $lang" >&2
                            exit 2
                            ;;
                    esac
                    case "$case" in
                        talker|listener) variant_offset=0 ;;
                        service-server|service-client) variant_offset=10 ;;
                        action-server|action-client) variant_offset=20 ;;
                        *)
                            echo "Unknown Zephyr fixture role: $case" >&2
                            exit 2
                            ;;
                    esac
                    xrce_port=$((2018 + lang_offset + variant_offset))
                fi
                entries+=(
                  "native_sim/native/64|build-${lang_tag}-${case}-${rmw}|zephyr/${lang}/${case}|0|$xrce_port|prj.conf;prj-${rmw}.conf;$native_sim_nsos_conf"
                )
            done
        done
    done
    # `service-client-async` zephyr/rust fixture dropped 2026-06-02
    # per Phase 212.M-F.5 — pending async-`Component` trait decision.

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
        zenoh_locator=""
        if [[ "$build_dir" == *-zenoh ]]; then
            src_tail="${src#zephyr/}"
            fixture_lang="${src_tail%%/*}"
            fixture_role="${src_tail#*/}"
            case "$fixture_lang" in
                rust) lang_offset=0 ;;
                c) lang_offset=100 ;;
                cpp) lang_offset=200 ;;
                *)
                    echo "Unknown Zephyr fixture language in $src" >&2
                    exit 2
                    ;;
            esac
            case "$fixture_role" in
                talker|listener) variant_offset=0 ;;
                service-server|service-client) variant_offset=10 ;;
                action-server|action-client) variant_offset=20 ;;
                *)
                    echo "Unknown Zephyr fixture role in $src" >&2
                    exit 2
                    ;;
            esac
            zenoh_port=$((7456 + lang_offset + variant_offset))
            zenoh_locator="tcp/127.0.0.1:$zenoh_port"
            extra_args+=(-DCONFIG_NROS_ZENOH_LOCATOR="\"$zenoh_locator\"")
        fi
        # Cyclone (native_sim): bake a distinct ROS_DOMAIN_ID per communicating
        # role-set so the native_sim Cyclone test groups run in PARALLEL. Cyclone
        # derives its RTPS ports from the domain (7400 + 250*domain); a talker+
        # listener / server+client pair shares one domain (same lang+variant),
        # unrelated sets differ → no cross-test port collision. Compile-time
        # (Kconfig), matching the embedded model — no runtime arg or env.
        if [[ "$build_dir" == *-cyclonedds ]]; then
            src_tail="${src#zephyr/}"
            fixture_lang="${src_tail%%/*}"
            fixture_role="${src_tail#*/}"
            case "$fixture_lang" in
                rust) lang_idx=0 ;;
                c) lang_idx=1 ;;
                cpp) lang_idx=2 ;;
                *) echo "Unknown Zephyr fixture language in $src" >&2; exit 2 ;;
            esac
            case "$fixture_role" in
                talker|listener) variant_idx=0 ;;
                service-server|service-client) variant_idx=1 ;;
                action-server|action-client) variant_idx=2 ;;
                *) echo "Unknown Zephyr fixture role in $src" >&2; exit 2 ;;
            esac
            # 50..58: 3 langs x 3 variants, all <= 232 (DDS-valid).
            cyclone_domain=$((50 + lang_idx * 3 + variant_idx))
            extra_args+=(-DCONFIG_NROS_DOMAIN_ID="$cyclone_domain")
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
            "zenoh_locator=$zenoh_locator" \
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
        # 184.9 — guard against stale incremental builds. Bare `ninja -C` has
        # intermittently failed to pick up a cyclone-backend edit (a churned/
        # half-failed build dir reuses a stale object → stale binary), which
        # silently broke 184.8 backend diagnosis (a probe ran against an old
        # binary, even hiding a compile error). For cyclonedds fixtures, if any
        # `nros-rmw-cyclonedds/src` source is newer than this fixture's linked
        # binary, force a clean west reconfigure so backend edits always land.
        # Fires only right after a backend edit; no effect on normal builds or
        # on non-cyclone fixtures.
        eff_pristine="$pristine"
        case "$actual_build_dir" in
            *cyclonedds*)
                if [ -f "$actual_build_dir/zephyr/zephyr.exe" ] && [ -n "$(find \
                        "$nros_root/packages/dds/nros-rmw-cyclonedds/src" \
                        \( -name '*.cpp' -o -name '*.hpp' \) \
                        -newer "$actual_build_dir/zephyr/zephyr.exe" -print -quit 2>/dev/null)" ]; then
                    needs_west=1
                    eff_pristine=always
                fi
                ;;
        esac
        if [ "$eff_pristine" != "always" ] && [ -f "$actual_build_dir/build.ninja" ] && [ "$needs_west" = "0" ]; then
            use_west=0
            if [ "${NROS_JOBSERVER:-}" = "1" ]; then
                build_argv=(ninja -C "$actual_build_dir")
            else
                build_argv=(ninja -C "$actual_build_dir" -j "$ninja_jobs")
            fi
        else
            use_west=1
            build_argv=(west build -b "$board" -d "$actual_build_dir" -p "$eff_pristine"
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
    # Phase 177.9 — the logging-smoke native_sim fixture is consumed by
    # logging_smoke.rs but lives under packages/testing/.../bins (not
    # examples/), so it's outside the entries loop above. build-all puts
    # zephyr in its fixture fanout, so build it here too — otherwise
    # build-all is not a superset of test-all's fixture needs and
    # logging_smoke_zephyr_native_sim hard-fails "fixture not built".
    # Honor the same filter; count it toward `selected` so a
    # logging-only filter doesn't trip the "no fixtures matched" guard.
    want_logging=0
    if [ -z "$fixture_filter" ] || [[ "logging-smoke-zephyr-native-sim" =~ $fixture_filter ]]; then
        want_logging=1
    fi
    if [ "$selected" -eq 0 ] && [ "$want_logging" -eq 0 ]; then
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
    echo "Zephyr test fixtures built successfully."

# Phase 117.13 — Zephyr build smoke for the FVP Base_RevC AEMv8-R
# Cortex-A SMP target (Phase 117.10's reference platform). Reuses
# fixture — no new app required just to prove the toolchain +
# board config + nros module compile clean for AArch64. Skips
# cleanly when west / Zephyr workspace / aarch64-zephyr-elf SDK
# aren't available.
[group("examples")]
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
[group("examples")]
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
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
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
[group("examples")]
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
    # Unified log dir (test-logs/) instead of a repo-root scatter.
    mkdir -p "$nros_root/test-logs"
    s32z_log="$nros_root/test-logs/s32z-build.log"
    west build -b "s32z2xxdc2@D/s32z270/rtu0" \
        -d build-s32z-talker -p auto \
        -- -DEXTRA_CONF_FILE="$extra" -DMAKE="$make_bin" \
        > "$s32z_log" 2>&1
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
        echo "  Build log: $s32z_log"
        exit 1
    fi

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
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    echo "  Building logging-smoke-zephyr-native-sim -> build-logging-smoke/"
    west build -b native_sim/native/64 -d build-logging-smoke -p auto \
        $NROS/packages/testing/nros-tests/bins/logging-smoke-zephyr-native-sim \
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
[group("main")]
test verbose="":
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

# Start zenohd router for Zephyr examples (port 7456, all interfaces)
zenohd:
    zenohd --listen tcp/0.0.0.0:7456 --no-multicast-scouting
