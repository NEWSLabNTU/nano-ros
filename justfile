set dotenv-load

# Common clippy lints for real-time safety
CLIPPY_LINTS := "-D warnings -D clippy::infinite_iter -D clippy::while_immutable_condition -D clippy::never_loop -D clippy::empty_loop -D clippy::unconditional_recursion -W clippy::large_stack_arrays -W clippy::large_types_passed_by_value"

LOG_DIR := "test-logs"

# Default paths for external SDKs — exported so all recipes (build + test) see them
export FREERTOS_DIR := env("FREERTOS_DIR", justfile_directory() / "third-party/freertos/kernel")
export FREERTOS_PORT := env("FREERTOS_PORT", "GCC/ARM_CM3")
export LWIP_DIR := env("LWIP_DIR", justfile_directory() / "third-party/freertos/lwip")
export FREERTOS_CONFIG_DIR := env("FREERTOS_CONFIG_DIR", justfile_directory() / "packages/boards/nros-mps2-an385-freertos/config")
export NUTTX_DIR := env("NUTTX_DIR", justfile_directory() / "third-party/nuttx/nuttx")
export NUTTX_APPS_DIR := env("NUTTX_APPS_DIR", justfile_directory() / "third-party/nuttx/nuttx-apps")
export THREADX_DIR := env("THREADX_DIR", justfile_directory() / "third-party/threadx/kernel")
export THREADX_CONFIG_DIR := env("THREADX_CONFIG_DIR", justfile_directory() / "packages/boards/nros-threadx-linux/config")
export NETX_DIR := env("NETX_DIR", justfile_directory() / "third-party/threadx/netxduo")
export NETX_CONFIG_DIR := env("NETX_CONFIG_DIR", justfile_directory() / "packages/boards/nros-threadx-linux/config")

# =============================================================================
# Platform modules (just <platform> <recipe>)
# =============================================================================

mod freertos 'just/freertos.just'
mod nuttx 'just/nuttx.just'
mod threadx_linux 'just/threadx-linux.just'
mod threadx_riscv64 'just/threadx-riscv64.just'
mod zephyr 'just/zephyr.just'
mod esp32 'just/esp32.just'
mod qemu 'just/qemu-baremetal.just'
mod native 'just/native.just'
mod xrce 'just/xrce.just'
mod docker 'just/docker.just'
mod workspace 'just/workspace.just'
mod verification 'just/verification.just'
mod zenohd 'just/zenohd.just'

default:
    @just --list

# =============================================================================
# Entry Points
# =============================================================================

# Build everything: refresh bindings, workspace (native + embedded), all examples, and test deps
build: \
    install-local generate-bindings \
    build-workspace build-workspace-embedded \
    native::build \
    freertos::build threadx_linux::build threadx_riscv64::build \
    build-zenohd qemu::build-zenoh-pico
    @echo "All builds completed!"

# Populate build/install/ with C/C++ artifacts (libraries, headers, CMake module, codegen).
# Builds posix (zenoh + xrce) unconditionally, then platform-specific libraries when toolchains are available.
install-local: \
    install-local-posix \
    freertos::install nuttx::install \
    threadx_linux::install threadx_riscv64::install
    @echo "Installed to $(pwd)/build/install"

# Build POSIX host libraries + codegen tool (zenoh + xrce)
install-local-posix:
    #!/usr/bin/env bash
    set -e
    PREFIX="$(pwd)/build/install"
    for rmw in zenoh xrce; do
        echo "=== Building posix RMW=$rmw ==="
        cmake -S . -B "build/cmake-$rmw" \
            -DNANO_ROS_RMW="$rmw" \
            -DNANO_ROS_PLATFORM="posix" \
            -DCMAKE_BUILD_TYPE=Release
        cmake --build "build/cmake-$rmw"
        cmake --install "build/cmake-$rmw" --prefix "$PREFIX"
    done

# Remove the install prefix and rebuild from scratch.
# Use after library renames or CMake structural changes that leave stale files.
clean-install:
    rm -rf build/install/
    just install-local

# Create a combined binary distribution archive of the full install prefix.
# Runs install-local first, then archives build/install/ as a self-contained
# prefix tree. Extract and pass to cmake via -DCMAKE_PREFIX_PATH=<dir>.
#
# Output: nros-<version>-<os>-<arch>.tar.gz  (in the project root)
package:
    #!/usr/bin/env bash
    set -e
    just clean-install
    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)
    DIRNAME="nros-${VERSION}-${OS}-${ARCH}"
    ARCHIVE="${DIRNAME}.tar.gz"
    rm -rf "build/package"
    mkdir -p "build/package/${DIRNAME}"
    cp -a build/install/. "build/package/${DIRNAME}/"
    tar -czf "${ARCHIVE}" -C build/package "${DIRNAME}"
    echo "Created: ${ARCHIVE}"
    echo "Usage:   cmake -DCMAKE_PREFIX_PATH=\$(pwd)/${DIRNAME} ..."

# Format everything: Rust workspace + examples, C, C++, Python
format: format-workspace native::format format-c format-cpp format-python
    @echo "All formatting completed!"

# Check everything: Rust (native + embedded + features + examples), C, C++, Python
check: \
    check-workspace check-workspace-embedded check-workspace-features \
    native::check check-c check-cpp check-python
    @echo "All checks passed!"

# Run unit tests only (no external dependencies)
test-unit verbose="":
    #!/usr/bin/env bash
    args=(--workspace --exclude nros-tests --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

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

# Run standard tests (needs qemu-system-arm + zenohd)
# Single nextest run (workspace + integration, excluding zephyr/ros2/large_msg) + Miri
test verbose="": build-zenohd
    #!/usr/bin/env bash
    set +e
    failed=0
    args=(--workspace --no-fail-fast
          -E 'not binary(zephyr) and not binary(rmw_interop) and not binary(xrce_ros2_interop) and not binary(esp32_emulator) and not binary(large_msg) and not binary(nuttx_qemu) and not binary(threadx_linux) and not binary(threadx_riscv64_qemu)')
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
    echo "=== Miri ==="
    just test-miri || failed=1
    echo ""
    echo "JUnit XML: target/nextest/default/junit.xml"
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some tests failed."
        exit 1
    else
        echo "All standard tests passed!"
    fi

# Run all tests including Zephyr, ROS 2 interop, C API, XRCE, NuttX, FreeRTOS, large_msg
# Single nextest run (entire workspace) + Miri + C codegen
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

# Run CI: format check + clippy + tests (never modifies code)
ci: check test
    @echo "CI passed!"

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
# nros-c/nros-cpp excluded from no_std build: staticlib/cdylib requires panic handler (needs std)
build-workspace:
    cargo build --workspace --no-default-features --exclude nros-c --exclude nros-cpp
    cargo nextest run --workspace --no-run

# Build workspace for embedded target (Cortex-M4F)
# Excludes zpico-sys: requires native system headers for CMake build
# Excludes nros-tests: requires std (test framework dependencies)
# Excludes nros-c/nros-cpp: staticlib/cdylib requires panic handler (needs std)
build-workspace-embedded:
    cargo build --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zpico-sys \
        --exclude nros-tests \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-platform-posix \
        --exclude nros-platform-nuttx \
        --exclude zpico-platform-shim \
        --exclude xrce-platform-shim

# Format workspace code
format-workspace:
    cargo +nightly fmt

# Check workspace: formatting and clippy (no_std, native)
# nros-c/nros-cpp excluded from no_std check: staticlib/cdylib requires panic handler (needs std)
check-workspace:
    cargo +nightly fmt --check
    cargo clippy --workspace --no-default-features --exclude nros-c --exclude nros-cpp -- {{CLIPPY_LINTS}}

# Check workspace for embedded target (Cortex-M4F)
# Excludes zpico-sys: requires native system headers for CMake build
# Excludes nros-tests: requires std (test framework dependencies)
# Excludes nros-c/nros-cpp: staticlib/cdylib requires panic handler (needs std)
check-workspace-embedded:
    @echo "Checking workspace for embedded target..."
    cargo clippy --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zpico-sys \
        --exclude nros-tests \
        --exclude nros-c \
        --exclude nros-cpp \
        --exclude nros-platform-posix \
        --exclude nros-platform-nuttx \
        --exclude zpico-platform-shim \
        --exclude xrce-platform-shim -- {{CLIPPY_LINTS}}

# Check workspace with various feature combinations
check-workspace-features:
    @echo "Checking feature combinations..."
    @echo "  - nros: zenoh + posix + humble"
    cargo clippy -p nros --no-default-features --features "std,rmw-zenoh,platform-posix,ros-humble" -- {{CLIPPY_LINTS}}
    @echo "  - nros: zenoh + posix + iron"
    cargo clippy -p nros --no-default-features --features "std,rmw-zenoh,platform-posix,ros-iron" -- {{CLIPPY_LINTS}}
    @echo "  - nros-c: zenoh + posix + humble"
    cargo clippy -p nros-c --no-default-features --features "std,rmw-zenoh,platform-posix,ros-humble" -- {{CLIPPY_LINTS}}
    @echo "  - nros: cffi (no_std)"
    cargo clippy -p nros --no-default-features --features "rmw-cffi" -- {{CLIPPY_LINTS}}
    @echo "  - transport: sync-critical-section"
    cargo clippy -p nros-rmw --no-default-features --features "sync-critical-section" --target thumbv7em-none-eabihf -- {{CLIPPY_LINTS}}
    @echo "  - zenoh transport (std)"
    cargo clippy -p nros-rmw --features "std" -- {{CLIPPY_LINTS}}
    @echo "All feature checks passed!"

# Format C code (nros-c headers, zpico C, C examples) with clang-format
format-c:
    #!/usr/bin/env bash
    set -e
    echo "Formatting C code..."
    find packages/core/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 clang-format -i
    clang-format -i packages/zpico/zpico-zephyr/src/*.c packages/zpico/zpico-zephyr/include/*.h
    clang-format -i packages/zpico/zpico-smoltcp/c/*.c packages/zpico/zpico-smoltcp/c/*.h
    find examples/native/c -name '*.c' -not -path '*/build/*' -print0 | xargs -0 clang-format -i
    echo "C code formatted."

# Format C++ headers (nros-cpp) with clang-format
format-cpp:
    @echo "Formatting C++ headers..."
    clang-format -i packages/core/nros-cpp/include/nros/*.hpp
    @echo "C++ headers formatted."

# Format Python code (colcon-cargo-ros2) with ruff
format-python:
    @echo "Formatting Python code..."
    ruff format packages/codegen/packages/colcon-cargo-ros2/
    ruff check --fix packages/codegen/packages/colcon-cargo-ros2/
    @echo "Python code formatted."

# Check C code: formatting + nros-c umbrella header syntax
check-c:
    #!/usr/bin/env bash
    set -e
    echo "Checking C code..."
    echo "  - clang-format (nros-c headers)"
    find packages/core/nros-c/include -name '*.h' -not -name 'nros_generated.h' -print0 | xargs -0 clang-format --dry-run --Werror
    echo "  - clang-format (zpico C)"
    clang-format --dry-run --Werror packages/zpico/zpico-zephyr/src/*.c packages/zpico/zpico-zephyr/include/*.h \
        packages/zpico/zpico-smoltcp/c/*.c packages/zpico/zpico-smoltcp/c/*.h
    echo "  - clang-format (C examples)"
    find examples/native/c -name '*.c' -not -path '*/build/*' -print0 | xargs -0 clang-format --dry-run --Werror
    echo "  - syntax (nros-c umbrella header)"
    cc -fsyntax-only \
        -Ipackages/core/nros-c/include \
        -include packages/core/nros-c/include/nros/nros.h \
        -x c /dev/null
    echo "All C checks passed!"

# Check C++ headers: formatting + freestanding syntax + nros-cpp clippy
check-cpp:
    #!/usr/bin/env bash
    set -e
    echo "Checking C++ headers..."
    echo "  - clang-format"
    clang-format --dry-run --Werror packages/core/nros-cpp/include/nros/*.hpp
    echo "  - freestanding syntax (c++14)"
    for hdr in packages/core/nros-cpp/include/nros/*.hpp; do
        c++ -fsyntax-only -std=c++14 -ffreestanding -fno-exceptions -fno-rtti \
            -Ipackages/core/nros-cpp/include \
            -include "$hdr" -x c++ /dev/null
    done
    echo "  - nros-cpp clippy (zenoh + posix + humble)"
    cargo clippy -p nros-cpp --features "rmw-zenoh,platform-posix,ros-humble" -- {{CLIPPY_LINTS}}
    echo "All C++ checks passed!"

# Check Python code: formatting + linting with ruff
check-python:
    @echo "Checking Python code..."
    ruff format --check packages/codegen/packages/colcon-cargo-ros2/
    ruff check packages/codegen/packages/colcon-cargo-ros2/
    @echo "All Python checks passed!"

# Alias for test-unit (backward compatibility)
test-workspace verbose="": (test-unit verbose)

# Run Miri to detect undefined behavior in embedded-safe crates (no FFI)
test-miri:
    @echo "Running Miri on embedded-safe crates..."
    CARGO_PROFILE_DEV_OPT_LEVEL=0 cargo +nightly miri test -p nros-serdes -p nros-core -p nros-params


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
check-stack example="examples/qemu-arm-baremetal/rust/core/wcet-bench" top="30":
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
        examples/qemu-arm-baremetal/rust/core/wcet-bench \
        examples/qemu-arm-baremetal/rust/core/cdr-test \
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
    cargo +nightly llvm-cov clean --workspace

    for entry in "${CRATES[@]}"; do
        crate=$(echo "$entry" | awk '{print $1}')
        extra_args=$(echo "$entry" | cut -d' ' -sf2-)
        report_dir="$OUTPUT_DIR/$crate"
        mkdir -p "$report_dir"

        echo "--- $crate ---"

        # Try MC/DC first (--mcdc implies branch), fall back to branch-only
        # --no-clean preserves HTML from prior crate runs
        if cargo +nightly llvm-cov test --no-clean \
            -p "$crate" $extra_args \
            --mcdc \
            --html --output-dir "$report_dir" 2>/dev/null; then
            echo "  [OK] MC/DC + branch coverage → $report_dir/"
        else
            echo "  [INFO] MC/DC not supported on this toolchain, using branch coverage"
            cargo +nightly llvm-cov test --no-clean \
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
build-zenoh:
    cargo build -p nros-rmw --features std

# Check zenoh transport
check-zenoh:
    cargo clippy -p nros-rmw --features std -- {{CLIPPY_LINTS}}

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

    # Auto-discover all examples with package.xml (Rust only, not zephyr)
    for pkg in $(find examples -name package.xml -not -path '*/target/*' -not -path '*/generated/*' | sort); do
        dir="$(dirname "$pkg")"
        echo "  $dir"
        (cd "$dir" && $NANO_ROS generate-rust)
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
    echo "All generated bindings removed."

# Regenerate rcl-interfaces bindings (workspace member with nros- prefix)
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
setup:
    @just _orchestrate setup

# Diagnose install status (read-only).
doctor:
    @just _orchestrate doctor

# Internal: walk every module calling the requested recipe (setup or doctor).
[private]
_orchestrate verb:
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
    run workspace
    run verification
    run qemu
    run freertos
    run nuttx
    run threadx_linux
    run threadx_riscv64
    run esp32
    run zephyr
    run xrce
    run zenohd
    echo ""
    if [ ${#failed[@]} -gt 0 ]; then
        echo "{{verb}} finished with ${#failed[@]} failure(s): ${failed[*]}"
        echo "Re-run individually: just <module> {{verb}}"
        exit 1
    fi
    echo "{{verb}} complete!"

# Setup all network bridges (QEMU + Zephyr, requires sudo)
setup-network: qemu::setup-network
    sudo ./scripts/zephyr/setup-network.sh

# Teardown all network bridges (requires sudo)
teardown-network: qemu::teardown-network
    sudo ./scripts/zephyr/setup-network.sh --down

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
    (cd packages/core/nros-c && doxygen Doxyfile)
    echo "C API docs generated: target/doc/c-api/html/index.html"

# Verify hand-written C headers are syntactically correct.
# Signature drift against Rust is caught at link time by `just test-c`.
doc-c-check:
    #!/usr/bin/env bash
    set -e
    echo "Checking C headers for syntax errors..."
    cc -fsyntax-only \
        -Ipackages/core/nros-c/include \
        -include packages/core/nros-c/include/nros/nros.h \
        -x c /dev/null
    echo "All C headers are syntactically correct."

# Generate all documentation (Rust + C + book)
doc: doc-rust doc-c

# Build the mdbook user guide
book:
    mdbook build book/

# Serve the mdbook with live reload
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

# Show Zephyr build instructions
zephyr-help:
    just zephyr help

# =============================================================================
# Docker: use `just docker build`, `just docker shell`, `just docker test`, etc.
# =============================================================================
