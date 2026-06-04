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
