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
    # issue 0599 — SKIPPED is its own verdict. These two used to `exit 0`, so a
    # host with no Zephyr workspace got `== zephyr == OK` for a lane that built
    # nothing, and the four west-owned compile-check fixtures went missing
    # twenty minutes downstream instead of being reported here.
    source scripts/build/lane-skip.sh
    trap 'for pid in $(jobs -pr); do kill "$pid" 2>/dev/null || true; done; wait; exit 130' INT TERM
    # issue 0698 follow-up — the Zephyr venv is this lane's, not the session's.
    source scripts/build/zephyr-python.sh
    nros_zephyr_activate
    if ! command -v west >/dev/null 2>&1; then
        nros_lane_skip "west not found — install it, or provision with \`just zephyr setup\`"
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        nros_lane_skip "{{ZEPHYR_WORKSPACE}} not set up — run \`just zephyr setup\`"
    fi
    # Phase 180.B — examples read the ROS interface package dirs from env
    # (no more hardcoded /opt/ros/humble/share/* PKG_DIR). Default to the
    # stock Humble install; override per package if your ROS lives elsewhere.
    # phase-327 / issue 0368 — one ladder for EVERY interface dir (fix the
    # class, not the first symptom: std_msgs was fixed alone and the next
    # fixture died on example_interfaces): prefer the ROS install, fall back
    # to the BUNDLED share tree so the cyclone zephyr cells configure on a
    # ROS-less host (the /opt/ros defaults used to bake dead .msg paths into
    # build.ninja).
    _nros_iface_dir() {
        # $1 = env var name, $2 = package name
        if [ -z "$(eval echo "\${$1:-}")" ]; then
            if [ -d "/opt/ros/humble/share/$2" ]; then
                export "$1=/opt/ros/humble/share/$2"
            else
                export "$1=$PWD/packages/cli/interfaces/$2"
            fi
        fi
    }
    _nros_iface_dir NROS_STD_MSGS_DIR std_msgs
    _nros_iface_dir NROS_EXAMPLE_INTERFACES_DIR example_interfaces
    _nros_iface_dir NROS_BUILTIN_INTERFACES_DIR builtin_interfaces
    _nros_iface_dir NROS_UNIQUE_IDENTIFIER_MSGS_DIR unique_identifier_msgs
    _nros_iface_dir NROS_ACTION_MSGS_DIR action_msgs
    # Phase 180.A — 4.4 line: run west via the Python 3.12 venv (find_package
    # Python3>=3.12) and skip the 3.7-only patch set. The proven 4.4 fixture
    # subset is zenoh native_sim, which needs none of these patches (NSOS
    # multicast/recvmsg re-anchors are Tasks 5-6, only the cyclonedds path).
    # issue #87 — the host-toolchain export used to live here (version-keyed,
    # so only 4.4). It is now set per-leaf in zephyr-fixture-run-one.sh, keyed
    # on the native_sim BOARD, so 3.7's native_sim subset is SDK-free too while
    # real embedded boards stay SDK-gated.
    venvbin="{{ZEPHYR_VENV_BIN}}"
    if [ -n "$venvbin" ]; then
        export PATH="$(realpath "$venvbin"):$PATH"
    fi
    if [ "{{NROS_ZEPHYR_VERSION}}" != "4.4" ]; then
    # Apply Cortex-A9 patches (idempotent; required for the
    # build-dds-a9-{talker,listener} entries below).
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    # Phase 117.10/13 — add AArch64 Cortex-A/R support to
    # zephyr-lang-rust so the FVP Base_RevC AEMv8-R SMP target
    # (Phase-117 Cyclone DDS reference) builds.
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    # issue 0432 — the gpio pair, without which the `zephyr` crate does not
    # compile for any board with gpio nodes in its devicetree.
    bash ./scripts/zephyr/zephyr-lang-rust-gpio-patch.sh "$workspace"
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
    # invokes `nros sync`, absent from the shipped 0.3.7 release.
    # Probe once; emit `[PREREQ]` + exit 0 if missing.
    nros_require_sync "$nros_cli"
    while IFS= read -r pkg; do
        dir="$(dirname "$pkg")"
        stamp="$dir/build/.nros-ws-sync.stamp"
        # Wipe `generated/` if the trait-surface hash has drifted.
        NROS_REPO_DIR="$(pwd)" nros_codegen_stamp_check_or_wipe "$dir"
        # phase-363 — CONTENT, not mtime. `[ "$pkg" -nt "$stamp" ]` re-armed a
        # sync for every leaf after any rebase, stash or branch switch, because
        # those rewrite tracked files with identical bytes. Same input set
        # (the package.xml), same fail-safe direction; only "changed" now means
        # changed.
        if [ "${NROS_ZEPHYR_GENERATE_RUST_FORCE:-0}" = "1" ] || ! nros_pkg_sync_stamp_fresh "$pkg" "$stamp" || [ ! -d "$dir/generated" ]; then
            echo "    sync: $dir"
            mkdir -p "$dir/build"
            # issue 0318 — `--no-metadata`, same reason as the `build-one`
            # rust/* branch in zephyr-dev.just, which is where that fix landed.
            # This second call site was missed, so the fixture lane still hit
            # it: the source-metadata refresh compiles a host probe that
            # path-deps the example, dragging in the `zephyr` crate, whose
            # build.rs needs `DOTCONFIG` from a Zephyr cmake configure that has
            # not run and cannot (this sync is its prerequisite). The probe is
            # INAPPLICABLE to a Zephyr leaf, not merely slow. Codegen — the
            # reason this preflight exists — is unaffected.
            NROS_REPO_DIR="$(pwd)" "$nros_cli" sync --no-metadata "$dir"
            NROS_REPO_DIR="$(pwd)" nros_codegen_stamp_write "$dir"
            # Record the CONTENT the sync was run against, so an identical
            # package.xml reads as fresh however its mtime moved.
            nros_pkg_sync_stamp_current "$pkg" > "$stamp"
        fi
    # `git ls-files`, not `find` — a package.xml is tracked, so this is an index
    # lookup. See scripts/regenerate-bindings.sh for the measurement that motivated
    # the rule (a pruned scan of examples/ cost 7m36s for what git returns in 0.8s).
    done < <(git ls-files 'examples/zephyr/rust/*/package.xml' | sort)
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
    # Remember whether the USER pinned the west-build concurrency: an explicit
    # NROS_ZEPHYR_BUILD_JOBS (e.g. =1, the serial escape hatch) must bound the
    # fifo-jobserver token pool too, while the computed budget/8 split is a
    # FALLBACK-model artifact that must NOT shrink the pool (it throttled the
    # whole family to budget/8 compile processes — 3 of 24 cores here).
    user_build_jobs="${NROS_ZEPHYR_BUILD_JOBS:-}"
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

    # Issue 0086 — under the parallel west fan-out (jobs > 1), concurrent
    # cargo → rustup component-ensure calls collide on the shared
    # `~/.rustup/downloads/<hash>.partial` staging file ("could not rename …
    # No such file"), which fails even ALREADY-installed targets' re-verification.
    # Pre-add the rust-std targets the Zephyr fixtures span SERIALLY here, so no
    # concurrent rustup fetch happens during the parallel stage. Idempotent
    # (already-installed → no-op exit 0). A CI image that pre-bakes the targets
    # never downloads; a fresh host downloads once, serially. Serial builds
    # (jobs=1) don't race, so skip.
    if [ "$jobs" -gt 1 ]; then
        # The pinned nightly (the zephyr no_std builds' toolchain) — same source
        # as the root justfile's NIGHTLY (not in this module's var scope).
        _nightly="$(awk '/^channel/ {gsub(/"/,"",$3); print $3; exit}' "$nros_root/tools/rust-toolchain.toml")"
        echo "  → pre-adding rust-std targets serially (issue 0086 rustup race guard, +$_nightly)"
        for _tgt in x86_64-unknown-none armv7a-none-eabi armv7a-none-eabihf; do
            rustup target add --toolchain "$_nightly" "$_tgt" \
                || { echo "rustup target add $_tgt (+$_nightly) failed" >&2; exit 1; }
        done
    fi

    # Phase 225.P.6 — workspace-Entry (Approach A) prep. Provision the
    # workspace message bindings + system bringup glue from the workspace
    # ROOT (not the per-package `examples/zephyr/rust/*` loop above),
    # mirroring the native lane in scripts/build/workspace-fixtures-build.sh.
    # `nros sync` is the platform-agnostic codegen step; `codegen-system`
    # materializes the demo_bringup glue. The west build of the Entry leaf
    # (emitted below via --include-workspace-entry) consumes these.
    if [ -d examples/workspaces/rust/src/zephyr_entry ]; then
        echo "  → workspace-entry prep (nros sync + codegen-system)"
        ( cd examples/workspaces/rust
          NROS_REPO_DIR="$nros_root" "$nros_cli" sync >/dev/null
          NROS_REPO_DIR="$nros_root" "$nros_cli" codegen-system --bringup src/demo_bringup --out build-fixtures/demo_bringup >/dev/null )
    fi
    # phase-276 W1 (#128) — the parameterised workspace's Zephyr Entry. Pure-cargo
    # `nros::main!` path: `nros sync` provisions generated/ + the patch table; NO
    # codegen-system (the proc-macro bakes the system + the launch `<param>`
    # initials, same as the native ws-params lane).
    if [ -d examples/workspaces/features/src/zephyr_rust_params_entry ]; then
        echo "  → ws-params workspace-entry prep (nros sync)"
        ( cd examples/workspaces/features
          NROS_REPO_DIR="$nros_root" "$nros_cli" sync >/dev/null )
    fi
    # phase-276 W3 (#128) — the managed (lifecycle) workspace's Zephyr Entry.
    # Same pure-cargo `nros::main!` path as ws-params: `nros sync` only.
    if [ -d examples/workspaces/features/src/zephyr_rust_lifecycle_entry ]; then
        echo "  → ws-lifecycle workspace-entry prep (nros sync)"
        ( cd examples/workspaces/features
          NROS_REPO_DIR="$nros_root" "$nros_cli" sync >/dev/null )
    fi
    # phase-276 W5 — the QoS-override workspace's Zephyr Entry (nros sync only).
    if [ -d examples/workspaces/features/src/zephyr_rust_qos_entry ]; then
        echo "  → ws-qos workspace-entry prep (nros sync)"
        ( cd examples/workspaces/features
          NROS_REPO_DIR="$nros_root" "$nros_cli" sync >/dev/null )
    fi
    # phase-276 W4 — the E2E-safety (CRC) workspace's Zephyr Entry (nros sync only).
    if [ -d examples/workspaces/safety/src/zephyr_rust_safety_entry ]; then
        echo "  → safety workspace-entry prep (nros sync)"
        ( cd examples/workspaces/safety
          NROS_REPO_DIR="$nros_root" "$nros_cli" sync >/dev/null )
    fi
    # phase-276 W2 / #128 half 2 — the RT-tiers workspace's Zephyr Entry (nros sync only).
    if [ -d examples/workspaces/realtime-rust/src/zephyr_entry ]; then
        echo "  → ws-realtime workspace-entry prep (nros sync)"
        ( cd examples/workspaces/realtime-rust
          NROS_REPO_DIR="$nros_root" "$nros_cli" sync >/dev/null )
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
        NROS_ZEPHYR_JOBSERVER_TOKENS="${user_build_jobs:-$zephyr_budget}" \
        NROS_ZEPHYR_NINJA_JOBS="$ninja_jobs" \
        NROS_ZEPHYR_SCCACHE_DISABLE="$sccache_disable" \
        NROS_ZEPHYR_CCACHE_DIR="$ccache_dir" \
        NROS_ZEPHYR_CCACHE_TEMPDIR="$ccache_tmpdir" \
            scripts/build/zephyr-fixture-make-driver.sh "${zephyr_leaf_args[@]}"
    fi
    # issue 0549 — the logging-smoke leaf used to be built HERE by a second
    # recipe, because "it lives under packages/testing/.../bins, not examples/,
    # so it's outside the entries loop above". phase-350 W1 made it a manifest
    # row, which ended that: the leaf goes through the same driver as every
    # other one, honours the same filter, and writes the same
    # `build-logging-smoke` dir the test reads. The second builder is gone.
    if [ "$normal_records" = "0" ]; then
        echo "  ✗ no Zephyr fixtures matched NROS_ZEPHYR_FIXTURE_FILTER=$fixture_filter"
        exit 1
    fi
    if [ ${#failures[@]} -gt 0 ]; then
        printf '  ✗ failed: %s\n' "${failures[@]}"
        exit 1
    fi
    # zephyr west build-fixtures (issue 0041): build the west bringup fixtures the
    # cli_bringup_zephyr / board_import tests consume, so they don't run west at
    # test time. Self-gates on west/ZEPHYR_BASE; non-fatal (a failed build — e.g.
    # board_import without the FVP SDK — leaves no stamp, and the test deselects/skips).
    # Pass the recipe's resolved workspace through as ZEPHYR_BASE so the script
    # uses the SAME tree these CI builds did (it also self-discovers, but this
    # keeps them in lockstep when NROS_ZEPHYR_WORKSPACE picked a non-default tree).
    # issue 0700 — NO `|| true`, same reasoning as `just/esp32.just`: the
    # builder distinguishes "unprovisioned host, nothing to build" (exit 0) from
    # "attempted and produced nothing" (non-zero), so discarding it here would
    # re-hide exactly the failures that distinction exists to surface.
    ZEPHYR_BASE="$workspace/zephyr" bash scripts/build/west-fixtures.sh
    echo "Zephyr test fixtures built successfully."
    # RFC-0079 §4.1 / issue 0766 — now that `.config` files exist, finish the
    # check `check::tier-priority-plan` DEFERS. Zephyr's reserved transport band
    # is derived per image, so this is the only place it CAN be judged, and a
    # deferral nobody discharges is just an unchecked pin with better wording.
    python3 scripts/check-tier-priority-plan-image.py
