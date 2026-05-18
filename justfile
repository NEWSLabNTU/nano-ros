set dotenv-load

# Workspace-wide clippy lint levels live in root `Cargo.toml` under
# `[workspace.lints]` (and per-crate `[lints] workspace = true`). The
# old `CLIPPY_LINTS` string passed through `--` is no longer needed.

# Opt-in rustc wrapper. When `sccache` is on `PATH`, every `cargo`
# invocation under any `just` recipe shares its compilation cache —
# big win across per-example builds that recompile the same
# `nros-core` / `heapless` / etc. crates over and over. When sccache
# is absent the variable is empty, which cargo treats as unset
# (verified on cargo 1.95).
export RUSTC_WRAPPER := `command -v sccache 2>/dev/null || true`

LOG_DIR := "test-logs"

# Pinned nightly channel for workspace tooling (fmt, miri, llvm-cov, build-std, emit-stack-sizes).
# Source of truth: tools/rust-toolchain.toml. Read via awk so the version
# is never duplicated into build scripts.
NIGHTLY := `awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml`

# Default paths for external SDKs — exported so all recipes (build + test) see them
export FREERTOS_DIR := env("FREERTOS_DIR", justfile_directory() / "third-party/freertos/kernel")
export FREERTOS_PORT := env("FREERTOS_PORT", "GCC/ARM_CM3")
export LWIP_DIR := env("LWIP_DIR", justfile_directory() / "third-party/freertos/lwip")
export FREERTOS_CONFIG_DIR := env("FREERTOS_CONFIG_DIR", justfile_directory() / "packages/boards/nros-board-mps2-an385-freertos/config")
export NUTTX_DIR := env("NUTTX_DIR", justfile_directory() / "third-party/nuttx/nuttx")
export NUTTX_APPS_DIR := env("NUTTX_APPS_DIR", justfile_directory() / "third-party/nuttx/nuttx-apps")
export THREADX_DIR := env("THREADX_DIR", justfile_directory() / "third-party/threadx/kernel")
export THREADX_CONFIG_DIR := env("THREADX_CONFIG_DIR", justfile_directory() / "packages/boards/nros-board-threadx-linux/config")
export NETX_DIR := env("NETX_DIR", justfile_directory() / "third-party/threadx/netxduo")
export NETX_CONFIG_DIR := env("NETX_CONFIG_DIR", justfile_directory() / "packages/boards/nros-board-threadx-linux/config")

# =============================================================================
# Platform modules (just <platform> <recipe>)
# =============================================================================

mod freertos 'just/freertos.just'
mod nuttx 'just/nuttx.just'
mod threadx_linux 'just/threadx-linux.just'
mod threadx_riscv64 'just/threadx-riscv64.just'
mod zephyr 'just/zephyr.just'
mod esp32 'just/esp32.just'
mod esp_idf 'just/esp_idf.just'
mod qemu 'just/qemu-baremetal.just'
mod stm32f4 'just/stm32f4.just'
mod native 'just/native.just'
mod xrce 'just/xrce.just'
mod docker 'just/docker.just'
mod workspace 'just/workspace.just'
mod verification 'just/verification.just'
mod zenohd 'just/zenohd.just'
mod rmw_zenoh 'just/rmw_zenoh.just'
mod px4 'just/px4.just'
mod orin_spe 'just/orin-spe.just'
mod cyclonedds 'just/cyclonedds.just'
mod platformio 'just/platformio.just'

default:
    @just --list

# Show every recipe including private/internal ones.
# Maintainer/CI flow. End users want `just --list`.
list-all:
    #!/usr/bin/env bash
    set -e
    awk '
        # Skip attribute lines, comments, blank, indented (recipe bodies).
        /^[[:space:]]/ || /^#/ || /^\[/ || /^$/ { next }
        # Recipe head: "name[ params]:" — capture the name.
        /^[a-zA-Z_][a-zA-Z0-9_-]*([[:space:]]|:|\*)/ {
            n = $1
            sub(/:.*/, "", n)
            print n
        }
    ' justfile | sort -u
    echo ""
    echo "(Run \`just <name>\` for any of these. Public subset: \`just --list\`.)"

# =============================================================================
# Entry Points
# =============================================================================

# Build tiers (each tier is a strict superset of the previous):
#
#   build               workspace (native + embedded) + transports (zenohd, zenoh-pico).
#                       Fast — typical dev iteration.
#   build-examples      `build` + every example crate + per-RTOS example builds
#                       (native, freertos, threadx_linux, threadx_riscv64).
#                       Use to verify the example matrix compiles.
#   build-test-fixtures Per-test staged binaries: feature variants
#                       (--target-dir target-tls / target-safety / target-zero-copy
#                       / target-large-buf) and C / C++ fixture binaries built via
#                       cmake. Required before `just test-all`.
#   build-all           = build-examples + build-test-fixtures. True superset.
#                       Slow — expect 15-40 min depending on machine.
#
# Default `build` recipe: refresh bindings + workspace + transports.
#
# Phase 140 — `install-local` removed; `add_subdirectory(<repo-root>)`
# is the only supported C/C++ consumption shape. CMake-driven crates
# build in-tree via Corrosion when an example invokes them.
build: \
    generate-bindings \
    build-workspace build-workspace-embedded \
    build-zenohd qemu::build-zenoh-pico
    @echo 'Workspace + transports built. Run "just build-examples" for example crates, "just build-test-fixtures" for `test-all` staging, or "just build-all" for everything.'

# `build` + every example crate + per-RTOS example builds (native,
# freertos, threadx_linux, threadx_riscv64). Use to verify the
# example matrix still compiles after a core change.
build-examples: build \
    native::build-examples \
    freertos::build-examples threadx_linux::build-examples threadx_riscv64::build-examples
    @echo "Workspace + examples built."

# True superset: workspace + every example + per-test fixture variants.
# Pre-populates everything `just test-all` consumes. Slow.
build-all: build-examples build-test-fixtures
    @echo "All builds completed (workspace + examples + test fixtures)."

# Internal: invalidate stale nros-* cargo fingerprints in a cmake build
# dir's per-build cargo cache when shared-core source content has
# changed since the last build.
#
# Why: corrosion gives each cmake build dir its own cargo target tree
# under `build/cmake-<rmw>/cargo/...`. That tree's fingerprint check
# is mtime-based — when a `git checkout`, `git stash pop`, or similar
# operation rewrites a file's content WITHOUT bumping mtime past the
# fingerprint's `invoked.timestamp`, cargo decides "clean", reuses the
# pre-edit `.rlib`, and the resulting `lib<...>.a` carries stale
# code into every linked binary (zephyr, freertos, …). Cost us a
# multi-hour debug on cpp/xrce action E2E (post Phase 96.1).
#
# This guard hashes every shared-core `.rs` file and compares against
# a stamp file under the cmake build dir. Hash changed → nuke
# `nros*` fingerprints under that build dir → next cargo invocation
# revalidates. Hash unchanged → no-op (~200 ms hashing only).
_cmake-cargo-stale-guard build_dir:
    #!/usr/bin/env bash
    set -e
    BUILD_DIR="{{build_dir}}"
    [ -d "$BUILD_DIR" ] || exit 0
    SRC_HASH=$(find \
        packages/core \
        packages/xrce/nros-rmw-xrce \
        packages/zpico/nros-rmw-zenoh \
        packages/dds/nros-rmw-dds \
        -name '*.rs' -type f -print0 2>/dev/null \
        | sort -z \
        | xargs -0 sha1sum 2>/dev/null \
        | sha1sum | cut -d' ' -f1)
    STAMP="$BUILD_DIR/.shared-cores-hash"
    LAST_HASH=$(cat "$STAMP" 2>/dev/null || true)
    if [ "$SRC_HASH" != "$LAST_HASH" ]; then
        echo "[stale-guard] shared-core source hash changed → invalidating nros-* fingerprints in $BUILD_DIR/cargo"
        find "$BUILD_DIR/cargo" -type d -path '*/.fingerprint/nros*' -exec rm -rf {} + 2>/dev/null || true
        echo "$SRC_HASH" > "$STAMP"
    fi

# The cmake build dirs hold their own cargo target tree
# (`build/cmake-<rmw>/cargo/...`) whose incremental cache can hand
# back stale `.rlib`s after edits to deeply-shared crates like
# `nros-node`. The Phase 140 `add_subdirectory` shape consumes nano-ros
# in-tree per-example, so the only persistent build dirs are the user's
# per-example `build/` directories; flush by removing those.

# Format everything: Rust workspace + examples, C, C++, Python
format: format-workspace native::format format-c format-cpp format-python
    @echo "All formatting completed!"

# Check everything: Rust (native + embedded + features + examples), C, C++, Python
check: \
    check-workspace check-workspace-embedded check-workspace-features \
    check-platform-abi-mirror check-decoupling \
    native::check check-c check-cpp check-python
    @echo "All checks passed!"

# Phase 121.4.b — verify <nros/platform.h> matches the Rust extern block
# and the `nros_platform_export_*!` macro emissions in nros-platform-cffi.
[private]
check-platform-abi-mirror:
    @bash scripts/check-platform-abi-mirror.sh

# Phase 134.5 — verify the in-tree zenoh staticlib's internal symbol
# parity. For every defined `_z_f_link_*_<transport>` wrapper, the
# matching `_z_*_<transport>` impl must also be defined. Pre-Phase-134
# the POSIX CMake path shipped wrappers without multicast impls and
# every C/C++ native link broke. Run after
# `cargo build -p nros-rmw-zenoh-staticlib --release`.
check-zenoh-archive:
    @bash scripts/check-zenoh-archive-symbols.sh target/release/libnros_rmw_zenoh_staticlib.a

# Phase 104.A.4 — assert `nros` + `nros-node` Cargo deps stay free of
# concrete RMW / platform crates. The umbrella must consume only the
# generic ABI (`nros-rmw-cffi` vtable + `nros-platform-cffi` C header);
# selecting a backend or platform is the outer build system's job.
#
# Today this guard is EXPECTED TO FAIL — Phase 104.A is the migration
# that brings it to green. Wire it as a required check once the
# migration completes.
check-decoupling:
    @bash scripts/check-decoupling.sh

# Test tiers (each tier is a strict superset of the previous):
#
#   test-unit         workspace lib/bin tests except nros-tests crate.
#                     ~5s, no external deps.
#   test-integration  nros-tests integration tests excluding heavy QEMU /
#                     Zephyr / ROS-2-interop groups. ~30s, needs zenohd.
#   test              = test-unit + test-integration. Default dev tier.
#                     No miri, no heavy QEMU/Zephyr.
#   test-doc          rustdoc doctests for the `nros` umbrella crate.
#   test-miri         Miri UB scan on embedded-safe crates. Standalone, ~min.
#   test-all          = test + heavy QEMU / Zephyr / threadx-linux /
#                     ros2-interop groups + test-doc + test-miri + C codegen.
#                     True superset, requires `just build-test-fixtures` first.
#
# Per-platform tests (just <plat> test|test-all|ci) are organized in
# the matching just/<plat>.just files — see CLAUDE.md for the matrix.

# Workspace lib/bin/unit tests, excluding the integration crate.
test-unit verbose="":
    #!/usr/bin/env bash
    # `nros-rmw-{zenoh,dds,xrce}-cffi` excluded for the same reason as
    # `check-workspace`: their `*Rmw` type imports are platform-feature
    # gated, and `cargo nextest run --workspace` activates no features.
    # Real coverage of these shims comes from their per-feature
    # invocations under `check-workspace-features`.
    args=(--workspace --exclude nros-tests \
          --exclude nros-rmw-xrce-cffi \
          --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# nros-tests integration tests, skipping heavy cross-compile / QEMU groups.
# Filters mirror the `test` recipe's `-E` predicate, just scoped to
# `package(nros-tests)` so the workspace unit tests aren't re-run.
test-integration verbose="": build-zenohd
    #!/usr/bin/env bash
    set -e
    exclude='not (group(=qemu-baremetal) or group(=qemu-baremetal-shared) or group(=qemu-freertos) or group(=qemu-nuttx) or group(=qemu-threadx-riscv) or group(=qemu-esp32) or group(=threadx-linux) or group(=qemu-zephyr) or group(=qemu-zephyr-xrce) or group(=ros2-interop) or group(=xrce_ros2_interop))'
    args=(-p nros-tests --no-fail-fast -E "$exclude")
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Shared helper: run a single nros-tests integration test binary with the
# standard verbose-flag handling. Used by per-platform `test` / `test-all`
# recipes in just/<platform>.just so the args/verbose boilerplate lives in
# one place.
_nextest-platform test_name verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test {{test_name}} --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run rustdoc doctests for the `nros` umbrella crate.
# Nextest does not execute doctests, so we run them separately.
# This catches drift between rustdoc examples and the real API.
test-doc:
    cargo test --doc -p nros

# Count real (non-[SKIPPED]) test failures from the latest junit.xml.
# Tests that panic with `[SKIPPED] ...` (via the nros_tests::skip! macro)
# are environment-conditional skips and excluded from the real failure count.
# Counts only `<failure ` entries whose `message=` attribute contains [SKIPPED],
# not raw `[SKIPPED]` strings (which also appear in `<system-err>`).
_count-real-failures:
    #!/usr/bin/env bash
    junit=target/nextest/default/junit.xml
    if [ ! -f "$junit" ]; then
        echo 0
        exit 0
    fi
    # `grep -c` prints 0 on no-match and exits 1, so no `|| echo 0` fallback
    # is needed — the fallback would double-emit "0\n0" and break $(( )).
    total=$(grep -c '<failure ' "$junit")
    # A failure is environment-skipped if its <failure> tag's content contains [SKIPPED].
    # We grep for `<failure ` lines plus the next line (the panic message body).
    skipped=$(grep -A1 '<failure ' "$junit" | grep -c '\[SKIPPED\]')
    real=$((total - skipped))
    if [ $real -lt 0 ]; then real=0; fi
    echo "$real"

# Print a one-line summary of test outcomes from junit.xml.
_test-summary:
    #!/usr/bin/env bash
    junit=target/nextest/default/junit.xml
    if [ ! -f "$junit" ]; then
        echo "No junit.xml found"
        exit 0
    fi
    total=$(grep -c '<failure ' "$junit")
    skipped=$(grep -A1 '<failure ' "$junit" | grep -c '\[SKIPPED\]')
    real=$((total - skipped))
    if [ $real -lt 0 ]; then real=0; fi
    if [ $skipped -gt 0 ]; then
        echo "Environment-skipped tests: $skipped (missing prerequisites)"
        grep -A1 '<failure ' "$junit" | grep -o '\[SKIPPED\][^<&]*' \
            | sort | uniq -c | sort -rn | sed 's/^/  /'
    fi
    echo "Real failures: $real / $total total failures"

# Default dev tier — workspace unit tests + integration tests, with
# heavy QEMU / Zephyr / ROS-2-interop groups skipped. Does NOT run
# Miri (use `test-miri` or `test-all` for that).
#
# Heavy groups are skipped via a CLI `-E` predicate keyed off nextest
# test-groups (`qemu-{baremetal,freertos,nuttx,threadx-riscv,esp32,zephyr}`,
# `threadx-linux`, `ros2-interop`, `xrce_ros2_interop`). New heavy
# binaries inherit the skip by assigning to one of those groups in
# `.config/nextest.toml`. `group(...)` is a CLI-only predicate
# (nextest 0.9.133+), so the list lives here rather than under a
# `[profile.fast]` default-filter.
test verbose="": build-zenohd
    #!/usr/bin/env bash
    set +e
    failed=0
    exclude='not (group(=qemu-baremetal) or group(=qemu-baremetal-shared) or group(=qemu-freertos) or group(=qemu-nuttx) or group(=qemu-threadx-riscv) or group(=qemu-esp32) or group(=threadx-linux) or group(=qemu-zephyr) or group(=qemu-zephyr-xrce) or group(=ros2-interop) or group(=xrce_ros2_interop))'
    args=(--workspace --no-fail-fast -E "$exclude")
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"
    nextest_exit=$?
    real_failures=$(just _count-real-failures)
    if [ "$nextest_exit" -ne 0 ] && [ "$real_failures" -gt 0 ]; then
        failed=1
    fi
    echo ""
    just _test-summary
    echo ""
    echo "JUnit XML: target/nextest/default/junit.xml"
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some tests failed."
        exit 1
    else
        echo "All standard tests passed! (Miri skipped — run \`just test-miri\` or \`just test-all\`.)"
    fi

# Pre-build every example binary the test suite reaches.
#
# The contract is: tests only verify a binary exists at a known path —
# they never compile fixtures themselves. This recipe is the build
# phase. Splitting the build phase from the test phase lets cargo/cmake
# use full host parallelism without competing with N concurrent QEMU +
# zenohd processes during the nextest run, which used to stretch a 14 s
# test out to 125 s under load. Run this before `just test-all`.
# Phase 150.F — `generate-bindings` precondition: every per-platform
# `build-fixtures` recipe assumes `generated/<pkg>/` is populated for
# each fixture crate. Without it `cargo build` fails on
# `unable to update generated/builtin_interfaces`. Make the dep
# explicit so `just build-test-fixtures` (and `just test-all` via
# the bench fixtures it consumes) is self-contained.
build-test-fixtures: generate-bindings build-zenoh-posix-fixture
    just native build-fixtures
    just qemu build-fixtures
    just freertos build-fixtures
    just nuttx build-fixtures
    just threadx_linux build-fixtures
    just threadx_riscv64 build-fixtures
    just zephyr build-fixtures
    just stm32f4 build-fixtures
    @echo "All test fixtures built."

# Phase 150.E rev3 — single deterministic fixture serving both
# `nros-tests::zenoh_header_parity` (reads the canonical
# `zenoh_generic_config.h`) and `nros-tests::zenoh_archive_symbols`
# (reads `libnros_rmw_zenoh_staticlib.a`). Both artefacts are
# products of `cargo build -p nros-rmw-zenoh-staticlib --features
# platform-posix`; bundle them into one dedicated --target-dir so
# the tests always read the POSIX-policy variant, not whichever
# feature set hit the shared workspace `target/` last (a cross-
# target `just threadx_riscv64 build-fixtures` would otherwise
# overwrite both with Phase 146.2 `LinkPolicy::threadx()` content).
#
# Output (deterministic — one `zpico-sys-<hash>` per --target-dir):
#   target-zenoh-fixture-posix/release/libnros_rmw_zenoh_staticlib.a
#   target-zenoh-fixture-posix/release/build/zpico-sys-*/out/
#       zenoh-config/zenoh_generic_config.h
#
# Tests discover these paths via the `NROS_TESTS_ZENOH_ARCHIVE`
# and `NROS_TESTS_ZENOH_HEADER` env vars when set (out-of-tree /
# CI override); otherwise walk this directory.
#
# `--release` matters: `zenoh_archive_symbols.rs` predates this
# recipe and was written against `target/release/`. Sticking to
# release keeps both tests symmetric and matches the archive-
# parity script's expectation.
build-zenoh-posix-fixture:
    cargo build --release \
        -p nros-rmw-zenoh-staticlib \
        --features platform-posix \
        --target-dir target-zenoh-fixture-posix

# Run all tests including Zephyr, ROS 2 interop, C API, XRCE, NuttX, FreeRTOS, large_msg
# Single nextest run (entire workspace) + Miri + C codegen
#
# Fixtures are NOT auto-built — run `just build-test-fixtures` first.
test-all verbose="": build-zenohd
    #!/usr/bin/env bash
    set +e
    failed=0
    just init-test-logs
    args=(--workspace --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"
    nextest_exit=$?
    real_failures=$(just _count-real-failures)
    if [ "$nextest_exit" -ne 0 ] && [ "$real_failures" -gt 0 ]; then
        failed=1
    fi
    echo ""
    just _test-summary
    echo ""
    echo "=== Doctests ==="
    just test-doc || failed=1
    echo ""
    echo "=== Miri ==="
    just test-miri || failed=1
    echo ""
    echo "=== C Codegen Tests ==="
    just native _test-c-codegen {{verbose}} || failed=1
    echo ""
    echo "JUnit XML:  target/nextest/default/junit.xml"
    echo "Other logs: {{LOG_DIR}}/latest/"
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some tests failed."
        exit 1
    else
        echo "All tests passed!"
    fi

# Phase 146.3 — embedded-RTOS Rust-link regression gate.
#
# `cargo build` of one Rust example per hosted-RTOS that ships an
# embedded zenoh-pico variant (FreeRTOS, NuttX, ThreadX-Linux).
# These three are the targets whose link-symbol drift between
# `platform_aliases.c`, the zenoh-pico vendor TUs, and the
# `LinkPolicy` mask surfaced as Phase 146 A/B/C. Catches the next
# regression of the same shape (duplicate `_z_task_*`, undefined
# `_z_*_serial_internal`, etc.) immediately during `just ci`
# rather than during `just test-all`'s full QEMU sweep.
#
# Best-effort: each RTOS's build skips cleanly if its cross
# toolchain or board crate prerequisites are absent.
rust-rtos-link-check:
    #!/usr/bin/env bash
    set -e
    echo "== Phase 146.3 — embedded-RTOS Rust link check =="
    if command -v arm-none-eabi-gcc >/dev/null; then
        echo "  freertos talker:"
        ( cd examples/qemu-arm-freertos/rust/zenoh/talker && cargo build --release ) >/dev/null
        echo "  nuttx talker:"
        ( cd examples/qemu-arm-nuttx/rust/zenoh/talker && cargo build --release ) >/dev/null
    else
        echo "  [SKIPPED] freertos + nuttx: arm-none-eabi-gcc not installed"
    fi
    echo "  threadx-linux talker:"
    ( cd examples/threadx-linux/rust/zenoh/talker && cargo build --release ) >/dev/null
    echo "Rust-RTOS link check OK."

# Run CI: format check + clippy + every test tier (never modifies code).
# `test-all` already covers test-doc + test-miri internally. Phase
# 117.16 — `cyclonedds::ci` runs the C++ Cyclone DDS RMW backend's
# CTest harnesses (entity smoke + POSIX E2E vs stock
# `rmw_cyclonedds_cpp`). Phase 146.3 adds the `rust-rtos-link-check`
# gate ahead of `test-all` so the embedded-RTOS link-symbol
# regression class surfaces immediately on `just ci`.
ci: check rust-rtos-link-check test-all cyclonedds-ci
    @echo "CI passed!"

# Cyclone DDS module CI step. Best-effort: skips cleanly when the
# pinned Cyclone submodule hasn't been initialised (typical for
# contributors not touching Phase 117). The `cyclonedds::ci` recipe
# itself fails hard on actual test failures.
cyclonedds-ci:
    #!/usr/bin/env bash
    set -e
    if [ ! -f third-party/dds/cyclonedds/CMakeLists.txt ]; then
        echo "Cyclone DDS skip: submodule not initialised"
        echo "  (run \`just cyclonedds setup\` to enable)"
        exit 0
    fi
    just cyclonedds ci

# =============================================================================
# Test Infrastructure
# =============================================================================

# Kill orphaned test processes from previous runs
test-kill-orphans:
    #!/usr/bin/env bash
    echo "Killing orphaned test processes..."
    pkill -9 -f 'zenohd.*--listen.*--no-multicast' 2>/dev/null || true
    pkill -9 -f 'nano-ros/examples/.*/target/' 2>/dev/null || true
    pkill -9 -f 'nano-ros/examples/.*/build/' 2>/dev/null || true
    pkill -9 -f 'MicroXRCEAgent' 2>/dev/null || true
    pkill -9 -f 'ros2 topic' 2>/dev/null || true
    pkill -9 -f 'ros2 service' 2>/dev/null || true
    pkill -9 -f 'ros2 action' 2>/dev/null || true
    echo "Done."

# Initialize timestamped log directory for non-nextest test output (QEMU, C)
[private]
init-test-logs:
    #!/usr/bin/env bash
    timestamp=$(date +%Y%m%d-%H%M%S)
    mkdir -p "{{LOG_DIR}}/$timestamp"
    ln -sfn "$timestamp" "{{LOG_DIR}}/latest"

# View JUnit XML test report (requires: npm install -g junit-cli-report-viewer)
test-report:
    @junit-cli-report-viewer target/nextest/default/junit.xml

# =============================================================================
# Workspace
# =============================================================================

# Build workspace (no_std, native)
# nros-c/nros-cpp and standalone RMW staticlib wrappers excluded from
# no_std native build: staticlib/cdylib requires panic handler unless a
# concrete platform feature supplies the right runtime.
[private]
build-workspace:
    cargo build --workspace --no-default-features \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-dds-staticlib \
        --exclude nros-rmw-zenoh-staticlib
    cargo nextest run --workspace --no-run

# Build workspace for embedded target (Cortex-M4F)
# Excludes zpico-sys: requires native system headers for CMake build
# Excludes nros-tests: requires std (test framework dependencies)
# Excludes nros-c/nros-cpp/standalone RMW staticlib wrappers:
# staticlib/cdylib requires a platform-specific panic/runtime setup.
[private]
build-workspace-embedded:
    cargo build --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zpico-sys \
        --exclude nros-tests \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-dds-staticlib \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-sizes-build \
        --exclude nros-rmw-xrce-cffi \
        --exclude nros-rmw-uorb \
        --exclude nros-px4

# Format workspace code
[private]
format-workspace:
    cargo +{{NIGHTLY}} fmt

# Check workspace: formatting and clippy (no_std, native)
# nros-c/nros-cpp/standalone RMW staticlib wrappers excluded from no_std
# check: staticlib/cdylib requires a platform-specific panic/runtime setup.
# nros-rmw-{zenoh,dds,xrce}-cffi excluded because their `*Rmw` type
# imports are platform-feature-gated by the underlying impl crate
# (e.g. `ZenohRmw` only exists when one of `platform-{posix,zephyr,…}`
# is on). `--no-default-features --workspace` strips every feature
# from every member at once, so the cffi shim's `RustBackendAdapter<R>`
# can't resolve its type parameter. Real consumers always specify
# a platform; the per-feature combinations are covered by
# `check-workspace-features` further down.
[private]
check-workspace:
    cargo +{{NIGHTLY}} fmt --check
    cargo clippy --workspace --no-default-features \
        --exclude nros-c --exclude nros-cpp \
        --exclude nros-rmw-dds-staticlib \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-rmw-xrce-cffi

# Check workspace for embedded target (Cortex-M4F)
# Excludes zpico-sys: requires native system headers for CMake build
# Excludes nros-tests: requires std (test framework dependencies)
# Excludes nros-c/nros-cpp/standalone RMW staticlib wrappers:
# staticlib/cdylib requires a platform-specific panic/runtime setup.
[private]
check-workspace-embedded:
    @echo "Checking workspace for embedded target..."
    cargo clippy --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zpico-sys \
        --exclude nros-tests \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-rmw-dds-staticlib \
        --exclude nros-rmw-zenoh-staticlib \
        --exclude nros-sizes-build \
        --exclude nros-rmw-xrce-cffi \

# Check workspace with various feature combinations
[private]
check-workspace-features:
    @echo "Checking feature combinations..."
    # Phase 128.C.3 — `nros/rmw-zenoh-cffi` feature deleted; the
    # umbrella now only carries `rmw-cffi`. Backend selection is
    # done by adding the matching `nros-rmw-<name>` dep.
    @echo "  - nros: cffi + posix + humble"
    cargo clippy -p nros --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble"
    @echo "  - nros: cffi + posix + iron"
    cargo clippy -p nros --no-default-features --features "std,rmw-cffi,platform-posix,ros-iron"
    @echo "  - nros-c: zenoh-cffi + posix + humble"
    cargo clippy -p nros-c --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble"
    @echo "  - nros: cffi (no_std)"
    cargo clippy -p nros --no-default-features --features "rmw-cffi"
    @echo "  - transport: sync-critical-section"
    cargo clippy -p nros-rmw --no-default-features --features "sync-critical-section" --target thumbv7em-none-eabihf
    @echo "  - nros-rmw (std)"
    cargo clippy -p nros-rmw --features "std"
    @echo "All feature checks passed!"

# Format C code (nros-c headers, zpico C, C examples) with clang-format
[private]
format-c:
    #!/usr/bin/env bash
    set -e
    echo "Formatting C code..."
    find packages/core/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 clang-format -i
    clang-format -i packages/zpico/zpico-zephyr/src/*.c packages/zpico/zpico-zephyr/include/*.h
    find examples/native/c -name '*.c' -not -path '*/build/*' -print0 | xargs -0 clang-format -i
    echo "C code formatted."

# Format C++ headers (nros-cpp) with clang-format
[private]
format-cpp:
    @echo "Formatting C++ headers..."
    clang-format -i packages/core/nros-cpp/include/nros/*.hpp
    @echo "C++ headers formatted."

# Format Python code (colcon-cargo-ros2) with ruff
[private]
format-python:
    @echo "Formatting Python code..."
    ruff format packages/codegen/packages/colcon-cargo-ros2/
    ruff check --fix packages/codegen/packages/colcon-cargo-ros2/
    @echo "Python code formatted."

# Check C code: formatting + nros-c umbrella header syntax
[private]
check-c:
    #!/usr/bin/env bash
    set -e
    echo "Checking C code..."
    echo "  - clang-format (nros-c headers)"
    find packages/core/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 clang-format --dry-run --Werror
    echo "  - clang-format (zpico C)"
    clang-format --dry-run --Werror packages/zpico/zpico-zephyr/src/*.c packages/zpico/zpico-zephyr/include/*.h
    echo "  - clang-format (C examples)"
    find examples/native/c -name '*.c' -not -path '*/build/*' -print0 | xargs -0 clang-format --dry-run --Werror
    echo "  - syntax (nros-c umbrella header)"
    # The per-variant `<nros/nros_config_generated.h>` (defining the
    # OPAQUE_U64S macros referenced by `<nros/nros_generated.h>`) is
    # emitted by `nros-c`'s build.rs into `target/nros-c-generated/`.
    # Build first so the syntax check has those macros; otherwise
    # the source-tree stub fires its `#error`.
    cargo build -p nros-c --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble" --quiet 2>/dev/null || true
    # Variant dir FIRST so its `nros_config_generated.h` (with the
    # real OPAQUE_U64S macros) wins over the source-tree stub.
    cc -fsyntax-only \
        -Itarget/nros-c-generated \
        -Ipackages/core/nros-c/include \
        -include packages/core/nros-c/include/nros/nros.h \
        -x c /dev/null
    echo "All C checks passed!"

# Check C++ headers: formatting + freestanding syntax + nros-cpp clippy
[private]
check-cpp:
    #!/usr/bin/env bash
    set -e
    echo "Checking C++ headers..."
    echo "  - clang-format"
    clang-format --dry-run --Werror packages/core/nros-cpp/include/nros/*.hpp
    echo "  - freestanding syntax (c++14)"
    # parameter.hpp re-exposes the C-side `nros_param_*` API from
    # nros-c, so the syntax probe needs nros-c on the include path too.
    # The per-variant `<nros/nros_cpp_config_generated.h>` (defining
    # `NROS_CPP_EXECUTOR_STORAGE_SIZE` and friends, referenced by
    # `executor.hpp`'s `uint8_t storage_[NROS_CPP_EXECUTOR_STORAGE_SIZE]`)
    # is emitted by `nros-cpp`'s build.rs into
    # `target/nros-cpp-generated/`. Same C-side header for nros-c.
    # Build both first; variant dirs go FIRST on the include path so
    # their real headers win over the source-tree stubs.
    cargo build -p nros-c -p nros-cpp --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble" --quiet 2>/dev/null || true
    for hdr in packages/core/nros-cpp/include/nros/*.hpp; do
        c++ -fsyntax-only -std=c++14 -ffreestanding -fno-exceptions -fno-rtti \
            -Itarget/nros-cpp-generated \
            -Itarget/nros-c-generated \
            -Ipackages/core/nros-cpp/include \
            -Ipackages/core/nros-c/include \
            -include "$hdr" -x c++ /dev/null
    done
    echo "  - nros-cpp clippy (zenoh-cffi + posix + humble)"
    cargo clippy -p nros-cpp --no-default-features --features "std,rmw-zenoh-cffi,platform-posix,ros-humble"
    echo "All C++ checks passed!"

# Check Python code: formatting + linting with ruff
[private]
check-python:
    @echo "Checking Python code..."
    ruff format --check packages/codegen/packages/colcon-cargo-ros2/
    ruff check packages/codegen/packages/colcon-cargo-ros2/
    @echo "All Python checks passed!"

# Run Miri to detect undefined behavior in embedded-safe crates (no FFI)
test-miri:
    @echo "Running Miri on embedded-safe crates..."
    CARGO_PROFILE_DEV_OPT_LEVEL=0 cargo +{{NIGHTLY}} miri test -p nros-serdes -p nros-core -p nros-params


# =============================================================================
# Static Analysis
# =============================================================================

# Inspect generated assembly for a function (requires cargo-show-asm)
# Usage: just show-asm <package> <function> [target]
# Examples:
#   just show-asm nros-serdes 'CdrWriter::write_string'
#   just show-asm nros-serdes 'CdrWriter::write_string' thumbv7m-none-eabi
#   just show-asm nros-core 'Duration::from_nanos'
show-asm pkg fn target="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(-p "{{pkg}}" --lib "{{fn}}" --rust)
    if [[ -n "{{target}}" ]]; then
        args+=(--target "{{target}}" --no-default-features)
    fi
    cargo asm "${args[@]}"

# Show llvm-mca throughput analysis for a function (requires cargo-show-asm)
# Usage: just show-asm-mca <package> <function> [target]
show-asm-mca pkg fn target="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(-p "{{pkg}}" --lib "{{fn}}" --mca)
    if [[ -n "{{target}}" ]]; then
        args+=(--target "{{target}}" --no-default-features)
    fi
    cargo asm "${args[@]}"

# List all non-inlined functions in a crate (useful for finding inspectable symbols)
# Usage: just show-asm-list <package> [target]
show-asm-list pkg target="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(-p "{{pkg}}" --lib)
    if [[ -n "{{target}}" ]]; then
        args+=(--target "{{target}}" --no-default-features)
    fi
    cargo asm "${args[@]}" || true

# Analyze per-function stack usage (requires nightly + llvm-tools)
# Usage: just check-stack [example-dir] [top]
# Default: examples/qemu/rs-wcet-bench, top 30
check-stack example="packages/testing/nros-bench/wcet-cycles-qemu" top="30":
    ./scripts/stack-analysis.sh {{example}} --top {{top}}

# Analyze stack usage of a pre-built ELF (e.g. Zephyr west build output)
# Usage: just check-stack-elf <path-to-elf> [top]
check-stack-elf elf top="30":
    ./scripts/stack-analysis.sh --elf {{elf}} --top {{top}}

# Analyze stack usage of C examples (requires cmake + gcc)
# Usage: just check-stack-c [example-dir] [top]
# Default: examples/native/c-talker, top 30
check-stack-c example="examples/native/c/zenoh/talker" top="30":
    ./scripts/stack-analysis-c.sh {{example}} --top {{top}}

# Analyze stack usage of all examples (requires nightly + llvm-tools + cmake)
# Covers: QEMU ARM, native Rust, and native C examples
# ESP32/STM32F4 excluded (need platform-specific SDKs)
check-stack-all top="10":
    #!/usr/bin/env bash
    set -euo pipefail
    failed=0
    # Rust examples (QEMU ARM — no exclude, show full picture)
    for example in \
        packages/testing/nros-bench/wcet-cycles-qemu \
        packages/testing/nros-tests/bins/cdr-roundtrip-qemu \
        examples/qemu-arm-baremetal/rust/zenoh/talker \
        examples/qemu-arm-baremetal/rust/zenoh/listener \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis.sh "$example" --top {{top}} || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    # Rust examples (native — exclude tracing/regex infrastructure noise)
    for example in \
        examples/native/rust/zenoh/talker \
        examples/native/rust/zenoh/listener \
        examples/native/rust/zenoh/custom-msg \
        examples/native/rust/zenoh/service-server \
        examples/native/rust/zenoh/service-client \
        examples/native/rust/zenoh/action-server \
        examples/native/rust/zenoh/action-client \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis.sh "$example" --top {{top}} --exclude "regex_automata|regex_syntax|aho_corasick|env_filter|env_logger|driftsort" || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    # C examples (native)
    for example in \
        examples/native/c/zenoh/talker \
        examples/native/c/zenoh/listener \
        examples/native/c/zenoh/custom-msg \
        examples/native/c/zenoh/baremetal-demo \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis-c.sh "$example" --top {{top}} || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    if [ "$failed" -gt 0 ]; then
        echo "[WARN] $failed example(s) failed"
        exit 1
    fi
    echo "[OK] All stack analyses complete"

# Run Kani bounded model checking on core crates (requires kani-verifier)
# Proves panic-freedom, roundtrip correctness, and bounded behavior
verify-kani:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Kani Verification ==="
    failed=0
    for crate in nros-serdes nros-core nros-params nros-ghost-types nros-node; do
        echo ""
        echo "--- Verifying $crate ---"
        cargo kani -p "$crate" || { echo "[FAIL] $crate"; failed=$((failed + 1)); }
    done
    echo ""
    echo "--- Verifying nros-c ---"
    cargo kani -p nros-c --features "rmw-zenoh,platform-posix,ros-humble" || { echo "[FAIL] nros-c"; failed=$((failed + 1)); }
    echo ""
    if [ "$failed" -gt 0 ]; then
        echo "[FAIL] $failed crate(s) failed verification"
        exit 1
    fi
    echo "[OK] All Kani proofs verified"

# Run Verus unbounded deductive verification (requires Verus toolchain)
# Proves properties for ALL inputs using Z3 SMT solver
verify-verus:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Verus Verification ==="
    VERUS_DIR="$(pwd)/tools"
    if [ ! -x "$VERUS_DIR/verus" ]; then
        echo "Verus not found at $VERUS_DIR/verus"
        echo "Run 'just verification verus' to install"
        exit 1
    fi
    export PATH="$VERUS_DIR:$PATH"
    cd packages/verification/nros-verification
    cargo verus verify
    echo "[OK] All Verus proofs verified"

# Verify Phase 118.E size-probe rigorization: cross-mode parity,
# cross-target build under isolated mode, concurrency soak.
verify-size-probe:
    bash packages/testing/nros-tests/tests/size_probe_verify.sh

# Run all verification: Kani bounded model checking + Verus deductive verification
verify: verify-kani verify-verus

# Run branch coverage on safety-critical crates (requires nightly + cargo-llvm-cov)
# MC/DC is attempted first; falls back to branch-only if unsupported
coverage:
    #!/usr/bin/env bash
    set -euo pipefail

    if ! command -v cargo-llvm-cov &>/dev/null; then
        echo "ERROR: cargo-llvm-cov not found. Install with: cargo install cargo-llvm-cov --locked"
        exit 1
    fi

    CRATES=("nros-rmw --features safety-e2e" "nros-serdes" "nros-core")
    OUTPUT_DIR="target/llvm-cov/html"

    echo "=== Branch Coverage (safety-critical crates) ==="
    echo ""

    # Clean once at start so --no-clean preserves each crate's HTML output
    cargo +{{NIGHTLY}} llvm-cov clean --workspace

    for entry in "${CRATES[@]}"; do
        crate=$(echo "$entry" | awk '{print $1}')
        extra_args=$(echo "$entry" | cut -d' ' -sf2-)
        report_dir="$OUTPUT_DIR/$crate"
        mkdir -p "$report_dir"

        echo "--- $crate ---"

        # Try MC/DC first (--mcdc implies branch), fall back to branch-only
        # --no-clean preserves HTML from prior crate runs
        if cargo +{{NIGHTLY}} llvm-cov test --no-clean \
            -p "$crate" $extra_args \
            --mcdc \
            --html --output-dir "$report_dir" 2>/dev/null; then
            echo "  [OK] MC/DC + branch coverage → $report_dir/"
        else
            echo "  [INFO] MC/DC not supported on this toolchain, using branch coverage"
            cargo +{{NIGHTLY}} llvm-cov test --no-clean \
                -p "$crate" $extra_args \
                --branch \
                --html --output-dir "$report_dir"
            echo "  [OK] Branch coverage → $report_dir/"
        fi
        echo ""
    done

    echo "=== Coverage reports: $OUTPUT_DIR/ ==="

# =============================================================================
# Zenoh
# =============================================================================

# Build zenoh transport
[private]
build-zenoh:
    cargo build -p nros-rmw --features std

# Check zenoh transport
[private]
check-zenoh:
    cargo clippy -p nros-rmw --features std

# Build zenohd from submodule (alias for `just zenohd build`).
build-zenohd: zenohd::build

# Clean zenohd build (alias for `just zenohd clean`).
clean-zenohd: zenohd::clean


# Build zenoh-pico C library (standalone, for debugging)
build-zenoh-pico:
    @echo "Building zenoh-pico..."
    cd packages/zpico/zpico-sys/zenoh-pico && mkdir -p build && cd build && cmake .. -DBUILD_SHARED_LIBS=OFF && make
    @echo "zenoh-pico built at: packages/zpico/zpico-sys/zenoh-pico/build"

# =============================================================================
# Benchmarks
# =============================================================================
# Message Bindings
# =============================================================================

# Install cargo-nano-ros (requires ROS 2 environment)
install-cargo-nano-ros:
    @echo "Installing cargo-nano-ros..."
    cargo install --path packages/codegen/packages/cargo-nano-ros --locked

# Regenerate Rust bindings in all examples and rcl-interfaces
# Uses bundled interfaces (std_msgs, builtin_interfaces) — no ROS 2 environment required
generate-bindings:
    #!/usr/bin/env bash
    set -e
    echo "Building nano-ros codegen tool..."
    cargo build --manifest-path packages/codegen/packages/Cargo.toml -p cargo-nano-ros --bin nano-ros
    NANO_ROS="$(pwd)/packages/codegen/packages/target/debug/nano-ros"
    echo "Regenerating Rust bindings..."

    # Internal crate (workspace member — manually maintained, do not auto-regenerate)
    # To update: run `cargo nano-ros generate-rust` in packages/interfaces/rcl-interfaces/
    # then apply nros- prefix rename to generated Cargo.toml and source files

    # Auto-discover all examples with package.xml (Rust only, not zephyr).
    # `--force` so a system `apt upgrade ros-humble-*-msgs` actually
    # propagates into the regenerated `generated/<pkg>/Cargo.toml`
    # version field. Without it the per-package skip-if-exists check
    # leaves the old crate version in place and downstream cargo
    # rebuilds reuse the stale rlib.
    for pkg in $(find examples -name package.xml -not -path '*/target/*' -not -path '*/generated/*' | sort); do
        dir="$(dirname "$pkg")"
        echo "  $dir"
        (cd "$dir" && $NANO_ROS generate-rust --force)
    done
    # Phase 131.B — bench/test-fixture crates relocated under packages/testing/
    # also ship a package.xml + generated/ tree.
    for pkg in $(find packages/testing/nros-bench packages/testing/nros-tests/bins packages/testing/nros-smoke \
                     -name package.xml -not -path '*/target/*' -not -path '*/generated/*' 2>/dev/null | sort); do
        dir="$(dirname "$pkg")"
        echo "  $dir"
        (cd "$dir" && $NANO_ROS generate-rust --force)
    done

    echo "All bindings regenerated!"

# Remove generated/ directories in examples (not rcl-interfaces — it's a workspace member)
clean-bindings:
    #!/usr/bin/env bash
    set -e
    echo "Removing generated bindings..."
    # Auto-discover all generated/ directories under examples/
    for d in $(find examples -name generated -type d -not -path '*/target/*' | sort); do
        rm -rf "$d"
        echo "  removed $d"
    done
    # Phase 131.B — relocated bench/test-fixture crates under packages/testing/
    for d in $(find packages/testing/nros-bench packages/testing/nros-tests/bins packages/testing/nros-smoke \
                    -name generated -type d -not -path '*/target/*' 2>/dev/null | sort); do
        rm -rf "$d"
        echo "  removed $d"
    done
    echo "All generated bindings removed."

# Regenerate rcl-interfaces bindings (workspace member with nros- prefix)
[private]
generate-rcl-interfaces:
    #!/usr/bin/env bash
    set -e
    echo "Building nano-ros codegen tool..."
    cargo build --manifest-path packages/codegen/packages/Cargo.toml -p cargo-nano-ros --bin nano-ros
    NANO_ROS="$(pwd)/packages/codegen/packages/target/debug/nano-ros"
    echo "Regenerating rcl-interfaces bindings..."
    cd packages/interfaces/rcl-interfaces
    rm -rf generated/humble/nros-builtin-interfaces generated/humble/nros-rcl-interfaces
    $NANO_ROS generate-rust --force -o generated/humble \
        --rename builtin_interfaces=nros-builtin-interfaces \
        --rename rcl_interfaces=nros-rcl-interfaces
    echo "✓ rcl-interfaces regenerated"

# Regenerate lifecycle-msgs bindings (workspace member with nros- prefix)
[private]
generate-lifecycle-msgs:
    #!/usr/bin/env bash
    set -e
    echo "Building nano-ros codegen tool..."
    cargo build --manifest-path packages/codegen/packages/Cargo.toml -p cargo-nano-ros --bin nano-ros
    NANO_ROS="$(pwd)/packages/codegen/packages/target/debug/nano-ros"
    echo "Regenerating lifecycle-msgs bindings..."
    cd packages/interfaces/lifecycle-msgs
    rm -rf generated/humble/nros-lifecycle-msgs
    $NANO_ROS generate-rust --force -o generated/humble \
        --rename lifecycle_msgs=nros-lifecycle-msgs
    echo "✓ lifecycle-msgs regenerated"
    echo "NOTE: re-apply workspace inheritance to the generated Cargo.toml"
    echo "      (version.workspace, edition.workspace, etc.) — see rcl-interfaces."

# Clean and regenerate all bindings from scratch
regenerate-bindings: clean-bindings generate-bindings

# =============================================================================
# Setup & Doctor orchestrators
#
# `just setup`  — idempotently install everything (workspace + platforms + services).
# `just doctor` — read-only diagnosis of install status.
#
# Each module has its own `setup`/`doctor` recipes. The orchestrator walks
# them all, treats individual failures as non-fatal, and prints a summary.
# Run any module independently: e.g. `just nuttx setup`, `just zephyr doctor`.
# =============================================================================

# Install everything: workspace + verification + all platforms + services.
#
# Phase 123.A.4 — optional positional `target` (e.g. `just setup
# posix-zenoh`) shim to `tools/setup.sh --target=<target>`. When
# `target` is empty (`just setup`), runs the full contributor
# orchestrator that walks every per-platform module.
# Phase 142 — tiered orchestrator. `tier` ∈ {minimal,default,extended}.
# `NROS_SETUP_TIER` env overrides the default when no positional arg
# is passed. See docs/contributing/sdk-tiers.md for tier criteria.
#
# Phase 123.A.4 — optional positional `target` (e.g. `just setup
# posix-zenoh`) shim to `tools/setup.sh --target=<target>` is retained:
# `just setup <target>` invokes the per-target SDK setup script
# (interactive flow). Tier orchestration applies only to the no-arg
# `just setup` / explicit `just setup tier=<tier>` form.
setup target="" tier="":
    #!/usr/bin/env bash
    set -e
    if [[ -n "{{target}}" ]]; then
        exec "$(pwd)/tools/setup.sh" --target="{{target}}"
    fi
    chosen_tier="{{tier}}"
    if [[ -z "$chosen_tier" ]]; then
        chosen_tier="${NROS_SETUP_TIER:-default}"
    fi
    just _orchestrate setup "$chosen_tier"

# Diagnose install status (read-only). Tier matches `just setup`.
doctor tier="":
    #!/usr/bin/env bash
    set -e
    chosen_tier="{{tier}}"
    if [[ -z "$chosen_tier" ]]; then
        chosen_tier="${NROS_SETUP_TIER:-default}"
    fi
    just _orchestrate doctor "$chosen_tier"

# Internal: walk every module in `tier` calling the requested recipe
# (setup or doctor). Tiers are strict supersets: minimal ⊂ default ⊂
# extended. Unknown tier exits non-zero so a typo doesn't silently
# pick the wrong module list.
[private]
_orchestrate verb tier="default":
    #!/usr/bin/env bash
    set +e
    failed=()
    run() {
        local mod=$1
        echo ""
        echo "=== $mod ==="
        if just "$mod" {{verb}}; then
            :
        else
            failed+=("$mod")
        fi
    }
    # Phase 142.6 — surface the qemu PPA upgrade prompt at end of
    # `just doctor` (qemu module's own doctor already prints it).
    capture_qemu_doctor=""
    case "{{tier}}" in
        minimal)
            run workspace
            run verification
            run zenohd
            ;;
        default)
            # minimal
            run workspace
            run verification
            run zenohd
            # + RTOS, embedded, support services
            run qemu
            run freertos
            run nuttx
            run threadx_linux
            run threadx_riscv64
            run esp32
            run zephyr
            run xrce
            run rmw_zenoh
            run orin_spe
            run cyclonedds
            run platformio
            ;;
        extended)
            # default
            run workspace
            run verification
            run zenohd
            run qemu
            run freertos
            run nuttx
            run threadx_linux
            run threadx_riscv64
            run esp32
            run zephyr
            run xrce
            run rmw_zenoh
            run orin_spe
            run cyclonedds
            run platformio
            # + heavy / private-SDK modules
            run esp_idf
            run px4
            ;;
        *)
            echo "unknown tier '{{tier}}' — expected one of: minimal, default, extended" >&2
            exit 2
            ;;
    esac
    echo ""
    # Phase 142.6 — repeat the qemu < 7.2 PPA hint at the end of
    # `just doctor` so users don't scroll past it during the qemu
    # block. Skipped for `setup` (it would just duplicate the
    # `just qemu setup` output) and for `minimal` (no qemu in
    # that tier). Best-effort: silent if qemu missing entirely.
    if [[ "{{verb}}" == "doctor" && "{{tier}}" != "minimal" ]]; then
        if command -v qemu-system-arm >/dev/null 2>&1; then
            ver=$(qemu-system-arm --version 2>/dev/null | head -1 | sed -E 's/^[^0-9]*([0-9]+\.[0-9]+).*/\1/')
            major=${ver%%.*}
            minor=${ver##*.}
            if [ -n "$ver" ] && ! { [ "$major" -gt 7 ] || { [ "$major" -eq 7 ] && [ "$minor" -ge 2 ]; }; }; then
                echo "================================================================="
                echo "  REMINDER — system qemu-system-arm is $ver (< 7.2)."
                echo "================================================================="
                echo "  NuttX DDS multi-instance + ThreadX RV64 DDS tests need"
                echo "  '-netdev dgram,local.type=unix,...' from QEMU 7.2+."
                echo ""
                echo "  Primary remedy (no sudo, portable): just qemu setup-qemu"
                echo ""
                if [ -f /etc/os-release ] && grep -q '^ID=ubuntu' /etc/os-release; then
                    echo "  Fallback (system-wide, requires sudo) — Canonical PPA:"
                    echo "    sudo add-apt-repository ppa:canonical-server/server-backports"
                    echo "    sudo apt update && sudo apt install qemu-system-arm"
                else
                    echo "  Fallback: build from source — https://www.qemu.org/download/#source"
                fi
                echo "================================================================="
                echo ""
            fi
        fi
    fi
    if [ ${#failed[@]} -gt 0 ]; then
        echo "{{verb}} finished with ${#failed[@]} failure(s): ${failed[*]}"
        echo "Re-run individually: just <module> {{verb}}"
        echo "(tier: {{tier}})"
        exit 1
    fi
    echo "{{verb}} complete! (tier: {{tier}})"

# Setup bridge network for ThreadX Linux sim (requires sudo; Zephyr native_sim uses NSOS and needs no bridge)
setup-network: qemu::setup-network

# Teardown bridge network (requires sudo)
teardown-network: qemu::teardown-network

# Generate Rust API documentation (rustdoc)
doc-rust:
    cargo doc --workspace --no-deps

# Generate C API documentation (Doxygen)
# Requires doxygen — skips with a warning if not installed.
# The generated header must exist (run `cargo build -p nros-c` first).
doc-c:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping C API docs."
        echo "Install with: sudo apt install doxygen"
        exit 0
    fi
    header="packages/core/nros-c/include/nros/nros_generated.h"
    if [ ! -f "$header" ]; then
        echo "Generated header not found, building nros-c first..."
        cargo build -p nros-c --features "rmw-zenoh,platform-posix,ros-humble"
    fi
    mkdir -p target/doxygen/c
    (cd packages/core/nros-c && doxygen Doxyfile)
    echo "C API docs generated: target/doxygen/c/html/index.html"

# Verify hand-written C headers are syntactically correct.
# Signature drift against Rust is caught at link time by `just test-c`.
[private]
doc-c-check:
    #!/usr/bin/env bash
    set -e
    echo "Checking C headers for syntax errors..."
    cc -fsyntax-only \
        -Ipackages/core/nros-c/include \
        -include packages/core/nros-c/include/nros/nros.h \
        -x c /dev/null
    echo "All C headers are syntactically correct."

# Generate C++ API documentation (Doxygen).
doc-cpp:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping C++ API docs."
        echo "Install with: sudo apt install doxygen"
        exit 0
    fi
    mkdir -p target/doxygen/cpp
    (cd packages/core/nros-cpp && doxygen Doxyfile)
    echo "C++ API docs generated: target/doxygen/cpp/html/index.html"

# Generate Doxygen for the RMW vtable (porter-facing).
[private]
doc-rmw-cffi:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping rmw-cffi docs."
        exit 0
    fi
    mkdir -p target/doxygen/rmw-cffi
    (cd packages/core/nros-rmw-cffi && doxygen Doxyfile)
    echo "rmw-cffi docs generated: target/doxygen/rmw-cffi/html/index.html"

# Generate Doxygen for the platform vtable (porter-facing). Triggers a
# build of nros-platform-cffi first so the cbindgen-emitted header
# exists.
[private]
doc-platform-cffi:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping platform-cffi docs."
        exit 0
    fi
    header="packages/core/nros-platform-cffi/include/nros/platform_vtable.h"
    if [ ! -f "$header" ]; then
        echo "Generated header not found, building nros-platform-cffi first..."
        cargo build -p nros-platform-cffi
    fi
    mkdir -p target/doxygen/platform-cffi
    (cd packages/core/nros-platform-cffi && doxygen Doxyfile)
    echo "platform-cffi docs generated: target/doxygen/platform-cffi/html/index.html"

# Generate all documentation (Rust + C + C++ + cffi vtables + book).
doc: doc-rust doc-c doc-cpp doc-rmw-cffi doc-platform-cffi

# Build mdBook + stage rustdoc/Doxygen output beneath book/book/api/.
# Mirrors the deploy-book.yml workflow so contributors can preview the
# full deployed site (book + native API docs) locally.
#
# `target/doc/` is wiped before `cargo doc` so prior `cargo doc --workspace`
# runs don't leak into the deployed rustdoc tree (everything under
# target/doc/ gets copied verbatim).
book:
    #!/usr/bin/env bash
    set -e
    rm -rf target/doc target/doxygen
    # `nros::Executor`, `nros::Promise`, `nros::Node`, etc. only re-export
    # under `cfg(any(rmw-zenoh, rmw-xrce, rmw-dds, rmw-cffi))`. Pass an
    # rmw + platform feature combo so the deployed rustdoc actually shows
    # the public-facing types (otherwise the reference stub's
    # `[Executor](struct.Executor.html)` link 404s).
    # nros-rmw-xrce is mutually exclusive with nros-rmw-zenoh (compile-
    # time mutex on `nros`), so it's not part of this invocation.
    cargo doc --no-deps \
        --features rmw-zenoh,platform-posix,ros-humble \
        -p nros \
        -p nros-rmw \
        -p nros-rmw-cffi \
        -p nros-rmw-zenoh \
        -p nros-platform-api \
        -p nros-platform-cffi
    just doc-c
    just doc-cpp
    just doc-rmw-cffi
    just doc-platform-cffi
    mdbook build book
    mkdir -p book/book/api
    rm -rf book/book/api/rust book/book/api/c book/book/api/cpp \
           book/book/api/rmw-cffi book/book/api/platform-cffi
    cp -r target/doc                          book/book/api/rust
    cp -r target/doxygen/c/html               book/book/api/c
    cp -r target/doxygen/cpp/html             book/book/api/cpp
    cp -r target/doxygen/rmw-cffi/html        book/book/api/rmw-cffi
    cp -r target/doxygen/platform-cffi/html   book/book/api/platform-cffi
    # rustdoc has no top-level index when invoked with multiple `-p`; stage
    # a tiny redirect so visiting `api/rust/` lands on the umbrella crate.
    cat > book/book/api/rust/index.html <<'HTML'
    <!doctype html>
    <meta http-equiv="refresh" content="0; url=nros/index.html">
    <link rel="canonical" href="nros/index.html">
    <p>Redirecting to <a href="nros/index.html">nros</a>…</p>
    HTML
    echo "Built: book/book/index.html (open with xdg-open book/book/index.html)"

# Serve mdBook with live reload (book chapters only — does not rebuild
# rustdoc/Doxygen API docs; use `just book` for the full deployed view).
book-serve:
    mdbook serve book/ --open

# Clean all build artifacts created by `just build`
clean: native::clean zephyr::clean clean-zenohd
    cargo clean
    # Clean codegen workspace (separate Cargo workspace, not covered by cargo clean)
    cargo clean --manifest-path packages/codegen/packages/Cargo.toml
    # Clean stale per-crate target/ dirs inside workspace members (left by standalone builds)
    find packages -maxdepth 4 -name target -type d -not -path '*/codegen/packages/*' -exec rm -rf {} + 2>/dev/null || true
    # Clean CMake build dirs inside examples (stale caches break rebuild)
    find examples -name build -type d -exec rm -rf {} + 2>/dev/null || true
    rm -rf build
    @echo "All build artifacts cleaned"

# Phase 23.2 — Build per-arch `libnanoros.a` for every Arduino ESP32
# chip variant via the ESP-IDF toolchain. Requires `just esp_idf setup`
# to have populated the IDF workspace. Targets default to
# esp32c3,esp32s3,esp32; override with `ARDUINO_LIB_TARGETS=…`.
build-arduino-libs:
    @bash scripts/arduino/build-libnanoros.sh

# Phase 23.2 — Assemble the distributable Arduino library zip from
# whatever is currently under `arduino/nros/`. Requires
# `just build-arduino-libs` to have populated the per-arch `.a` slots.
package-arduino:
    @bash scripts/arduino/package-arduino-lib.sh

# Phase 23.5d — Host transport-glue smoke test. Builds
# `arduino/nros/src/nros_arduino.cpp` against the mock WiFi.h /
# Arduino.h stubs and verifies `set_nanoros_wifi_transports` /
# `nanoros_ping` behave under a mocked WiFi-connected state. No
# ESP-IDF / QEMU / hardware required.
test-arduino-transport:
    cmake -S tests/arduino/test-transport-host -B build/arduino/test-transport-host
    cmake --build build/arduino/test-transport-host
    build/arduino/test-transport-host/test_transport_host

# Phase 23.5b — ESP-IDF / libnanoros boot smoke. Boots the
# `scripts/arduino/idf-builder/` ELF (linked against the per-arch
# libnanoros) in qemu-system-riscv32's `esp32c3` machine and
# asserts the placeholder `app_main` line prints. Verifies that
# every nano-ros symbol resolves at IDF link time without dragging
# zenoh's runtime path through QEMU (which would need TAP +
# zenohd). Requires `just esp_idf setup` + `just
# build-arduino-libs`.
test-arduino-qemu-boot:
    #!/usr/bin/env bash
    set -e
    bin=build/arduino/esp32c3
    if [[ ! -f "$bin/nano_ros_arduino_lib_builder.elf" ]]; then
        echo "build/arduino/esp32c3 missing — run \`just build-arduino-libs\` first" >&2
        exit 2
    fi
    source esp-idf-workspace/env.sh >/dev/null 2>&1
    esptool.py --chip esp32c3 merge_bin --output "$bin/flash_image.bin" \
        --flash_mode dio --flash_freq 80m --flash_size 2MB \
        0x0    "$bin/bootloader/bootloader.bin" \
        0x8000 "$bin/partition_table/partition-table.bin" \
        0x10000 "$bin/nano_ros_arduino_lib_builder.bin" >/dev/null
    truncate -s 2M "$bin/flash_image_2m.bin"
    dd if="$bin/flash_image.bin" of="$bin/flash_image_2m.bin" conv=notrunc status=none
    out=$(timeout 8 qemu-system-riscv32 -nographic -machine esp32c3 \
        -drive file="$bin/flash_image_2m.bin",if=mtd,format=raw \
        -global driver=esp32c3.gpio,property=strap_mode,value=0x08 2>&1 || true)
    if grep -q "nano-ros Arduino library builder" <<< "$out"; then
        echo "[PASS] libnanoros boots in qemu-system-riscv32 esp32c3"
    else
        echo "[FAIL] expected app_main line not found"; echo "$out" | tail -30; exit 1
    fi

# Show Zephyr build instructions
zephyr-help:
    just zephyr help

# =============================================================================
# Docker: use `just docker build`, `just docker shell`, `just docker test`, etc.
# =============================================================================
