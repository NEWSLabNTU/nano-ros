# Phase 85: Test-Suite Consolidation & Speedup

**Goal**: Reduce the 214-function integration-test matrix in
`packages/testing/nros-tests/tests/` to a single-source-of-truth layout
that's ~70% smaller and ~40% faster to run, without sacrificing any
platform coverage.

**Status**: Complete (2026-04-24). Work items 85.1–85.10 landed;
85.11 abandoned (UX-break churn not justified by the payoff — the
exclusion-list fix in 85.7 already achieves the practical goal of
making `just test` fast and well-defined). Residual test-count growth
(214 → 260 as new phases added coverage) and the "<100 functions"
acceptance target are carried forward by
[Phase 89 — test-suite triage and parallelism](./phase-89-test-suite-triage-and-parallelism.md).
**Priority**: Medium — the suite isn't broken, but the duplication is
compounding: every new platform / transport has to be added in three
places (build helper, build test, E2E test), every bug fix that changes
a sleep-based wait has to be replicated across four platform files, and
the per-file `OnceCell` caches rebuild the same cross-compiled binaries
multiple times per nextest run.
**Depends on**: None — can land in parallel with Phase 80 and Phase 84.
**Successor**: Phase 89 inherits the <100-functions target and any
remaining sleep-based pacing.

## Overview

### Current state (audited 2026-04-20)

The integration-test suite under
`packages/testing/nros-tests/tests/` has accumulated substantial
duplication as platforms were added one at a time. A detailed audit
found:

| Surface                             | Count | Notes                                                        |
|-------------------------------------|:-----:|--------------------------------------------------------------|
| Test functions                      |  214  | Across 30 `.rs` files                                        |
| Build-only tests (`test_X_builds`)  |  69   | One per (platform, example, language) tuple                  |
| Near-identical E2E test bodies      |  32   | Same assertions, different `QemuProcess::start_X_virt`       |
| Per-file `OnceCell<PathBuf>` caches |  30   | No cross-file sharing — same binary rebuilt per test file    |
| `sleep(Duration::from_secs(≥2))`    |  11+  | Mostly in c_api / cpp_api / safety_e2e; ~15–30s per run      |
| Nextest `max-threads=1` groups      |   8   | One per platform; mostly over-restrictive post-port-table    |
| `#[ignore]`'d tests                 |   2   | Both genuinely blocked (Phase 77 C++ action; XRCE forwarder) |

### What "the same test, 32 times" looks like

Taking `test_freertos_pubsub_e2e` and `test_nuttx_c_pubsub_e2e` side by
side: both launch two QEMU instances, both sleep ~10 s for
stabilization, both scrape stdout for a `"Received"` pattern, both kill
processes at the end. The *only* difference is the `start_freertos_virt`
vs. `start_nuttx_virt` call and which per-platform zenohd port they
open. Same shape repeats for:

- FreeRTOS: C / Rust / C++ × {pubsub, service, action} = 9 tests
- NuttX:    C / Rust / C++ × {pubsub, service, action} = 9 tests (3 C++ `#[ignore]`d upstream)
- ThreadX Linux: C / Rust / C++ × {pubsub, service, action} = 8 tests
- ThreadX RISC-V: C / Rust / C++ × {pubsub, service, action} = 7 tests

That's the 32-body cluster. Build-only tests have the same problem at
higher arity (4 platforms × ~5 examples × 3 languages = 60-ish tests
that each just call `build_X()` and assert the binary exists).

### Non-goals

- **No test deletion that reduces coverage.** Every platform / language
  combination currently tested stays covered.
- **No change to the `#[ignore]`'d tests' blockers** (Phase 77 async
  action client, XRCE forwarder). Phase 85 doesn't touch the reasons,
  just the test plumbing.
- **No touching the rustfmt / clippy / miri / doctest pipelines** —
  those already run cleanly and are orthogonal to the integration-test
  redundancy.

## Work Items

### Group A — Land today (quick wins, ~0 risk)

- [x] 85.1 — Merge `c_api.rs` + `cpp_api.rs` into `native_api.rs`
  - **Files**: `packages/testing/nros-tests/tests/c_api.rs`,
    `packages/testing/nros-tests/tests/cpp_api.rs` → new
    `packages/testing/nros-tests/tests/native_api.rs`
  - **Goal**: Consolidate with `#[rstest(language in ["c", "cpp"])]`.
    Both files have identical assertion shapes on different binaries;
    the duplication is mechanical. Saves ~100 LOC + one test binary.
  - **Coverage**: Every existing native C / C++ test scenario runs
    under the parametrised test with the same assertions.

- [x] 85.2 — Replace `sleep(Duration::from_secs(N))` with ready-probes
  - **Files**: `c_api.rs`, `cpp_api.rs`, `safety_e2e.rs` (and any other
    files surfaced by `rg 'sleep\(Duration::from_secs\([2-9]'`)
  - **Goal**: Swap stabilization sleeps for
    `nros_tests::wait_for_port(port, Duration)` or equivalent. zenohd
    binds before `ZenohRouter::start()` returns, so the sleep is a
    belt-and-suspenders wait for something that's already finished.
  - **Expected speedup**: 15–30 s per full run. More if we push the
    probe into `spawn` / `wait_for_output_pattern` helpers.

- [x] 85.3 — Delete the 60+ build-only `test_X_Y_builds` tests
  - **Files**: `freertos_qemu.rs`, `nuttx_qemu.rs`, `threadx_linux.rs`,
    `threadx_riscv64_qemu.rs`
  - **Goal**: Remove `test_X_talker_builds`, `test_X_listener_builds`,
    `test_X_service_*_builds`, `test_X_action_*_builds` functions.
    Keep one per-platform `test_X_detection` smoke test (already
    exists: `test_threadx_detection`, etc.) and the `#[ignore]`d NuttX
    C++ build tests (those *are* the feature gate for the upstream
    libc block).
  - **Coverage rationale**: Every E2E test invokes the same
    `build_X_example()` helper first; if the build fails, the E2E test
    fails before `sleep(10)`. A dedicated build-only test adds no
    signal for ~20 s of wasted build time per test function.

### Group B — Next PR (medium risk, touches test file structure)

- [x] 85.4 — Parametrise the E2E tests by (platform, variant, language)
  - **Files**: `freertos_qemu.rs`, `nuttx_qemu.rs`, `threadx_linux.rs`,
    `threadx_riscv64_qemu.rs` → new
    `packages/testing/nros-tests/tests/rtos_e2e.rs`
  - **Goal**: Collapse the 32-body E2E cluster into three parametrised
    test functions:
    ```rust
    #[rstest]
    fn test_rtos_pubsub_e2e(
        #[values(FREERTOS, NUTTX, THREADX_LINUX, THREADX_RV64)] platform: Platform,
        #[values(Lang::Rust, Lang::C, Lang::Cpp)] lang: Lang,
    ) { ... }
    // + _service_e2e, _action_e2e
    ```
    The platform-specific bits
    (`QemuProcess::start_freertos_virt` / `_nuttx_virt` / etc.) move
    behind a `Platform::start(binary)` method. The zenohd port comes
    from the existing `nros_tests::platform::*` table.
  - **Per-platform skips**: `#[ignore]` or `platform.supports(lang)`
    check where a platform doesn't build that language — e.g.,
    FreeRTOS has no XRCE C tests.
  - **Benefit**: One place to change the ready-probe, the timeout, the
    output pattern. Adding a new platform becomes a one-line
    `Platform` table entry instead of a new `*.rs` file.

- [x] 85.5 — Consolidate `OnceCell<PathBuf>` builders into a shared module
  - **Files**: new
    `packages/testing/nros-tests/src/fixtures/binaries.rs` module
    (the existing `fixtures/binaries.rs` grows a per-platform
    submodule), the per-platform `.rs` test files shed their static
    blocks.
  - **Goal**: One `OnceCell<PathBuf>` per (platform, example,
    language) triple, visible to every test binary in a nextest run.
    Right now `freertos_qemu.rs` and `emulator.rs` both rebuild the
    same FreeRTOS-QEMU binaries because their caches are independent.
  - **Nextest note**: nextest runs each `.rs` test binary in a
    separate process, so statics don't cross the binary boundary
    anyway. The win is *within* the rtos_e2e.rs file from 85.4: one
    build serves all (variant × language) test runs for a given
    platform.

- [x] 85.6 — Shrink the nextest `max-threads=1` group list
  - **Files**: `.config/nextest.toml`
  - **Goal**: Replace the 8 per-platform groups with two groups:
    - `qemu-serial` — everything that launches a QEMU instance and
      needs `max-threads = 1` (avoids virtio-net TAP/slirp contention)
    - default — native / POSIX / interop tests, parallel
  - The per-platform zenohd-port table (`nros_tests::platform`) and
    the per-test `ZenohRouter::start(port)` already solve the
    zenohd-port-collision problem that motivated the original
    per-platform groups; the only remaining reason for serialising is
    QEMU resource pressure on a single host, which one group captures.

### Group C — Design discussion (bigger, optional)

- [x] 85.7 — Drift-fix: centralise the `just test` exclusion list
  - **Files**: `justfile`, `.config/nextest.toml`
  - **Problem**: `just test`'s inline `-E 'not binary(zephyr) and
    not binary(rmw_interop) and …'` filter had drifted out of sync
    with the test matrix. Missing from the exclusion — therefore
    silently included despite being heavy — were `emulator` (QEMU
    bare-metal), `freertos_qemu`, `rtos_e2e` (the Phase 85.4
    parametrised binary), `params` (ROS 2 interop), and
    `dds_ros2_interop`. 210 tests were running when ~126 was the
    intent.
  - **Fix**: `just test` now invokes nextest with
    `-E 'not (group(=qemu-serial) or group(=ros2-interop) or
    group(=xrce_ros2_interop) or group(=large_msg))'` — the
    exclusion references test-groups by name, so the group override
    list in `.config/nextest.toml` is the single source of truth.
    `dds_ros2_interop` was added to the `ros2-interop` group as
    part of this change (it was previously groupless).
  - **Note on nextest's `group()` filter predicate**: nextest
    0.9.133 (shipped via [PR #3273](https://github.com/nextest-rs/nextest/pull/3273))
    added the `group(name-matcher)` filterset predicate, but it's
    **CLI-only** — it can't appear inside a `[profile.*]`
    `default-filter` or a per-test override's `filter`. That's why
    the group list lives in `justfile` rather than in a
    `[profile.fast]` config block. Reference:
    [filterset docs](https://nexte.st/docs/filtersets/reference/).
  - **Doesn't do**: rename / UX changes. See 85.11.

- [~] 85.11 — `just test` / `just test-all` / `just ci` rename pass —
      **abandoned 2026-04-24**
  - **Rationale for abandonment**: the practical goal of 85.11 (make
    `just test` fast and well-defined) was already achieved by 85.7's
    drift-fix — `just test` now runs the fast nextest profile via the
    group-excluding filterset, and `just test-all` runs the full
    matrix. The rename was UX polish on top of a working setup, with
    a real downside (muscle-memory break + coordination with any
    downstream CI that invokes `just test`). Not worth the churn.
  - **Files**: `justfile`, `just/*.just`, downstream CI workflows,
    any contributor docs that reference `just test`.
  - **Original goal** (kept for archival context): rename
    `just test` → `just test-fast` (host / POSIX only, the current
    fast-profile scope), `just test-all` → `just test` (the new
    default that also covers QEMU platforms). `just ci` uses
    `test-fast`; nightlies use `test`.
  - **Caveat**: This is a UX break for anyone who has `just test`
    in muscle memory. Pair the rename with a transitional alias
    (`just test: @echo "renamed to test-fast"; just test-fast`) and
    a deprecation note. Coordinate with any external CI workflow
    that invokes `just test` — the scope flip (fast → full) is the
    part that actually matters, so a partially-migrated downstream
    will start running the full matrix on PRs.
  - **Depends on**: 85.7 (drift-fix must land first so `just
    test-fast` is well-defined before the rename).

- [x] 85.8 — Port-table migration for non-RTOS tests
  - **Files**: `packages/testing/nros-tests/tests/zephyr.rs` (8 × `tcp/127.0.0.1:7456`),
    `packages/testing/nros-tests/tests/esp32_emulator.rs`
    (2 × `tcp/127.0.0.1:7448` — which didn't even match
    `platform::ESP32.zenohd_port = 7454`, so this was a latent bug
    as well as a DRY issue). Native-side tests (`native_api.rs`,
    `services.rs`, `actions.rs`) already use the `zenohd_unique`
    rstest fixture (ephemeral port), so they needed no migration —
    the originally-listed `platform::NATIVE` constant was redundant.
  - **Result**: Replaced hardcoded locator strings with
    `format!("tcp/127.0.0.1:{}", platform::ZEPHYR.zenohd_port)` and
    the ESP32 equivalent. The ESP32 assertion message was also
    reformatted to pull the port from the constant. Comments inside
    the files that still show specific port numbers left in place
    (cosmetic-only, no behavioural impact).

### Group D — Follow-ups surfaced by 85.4 (production bugs, not test plumbing)

Phase 85.4 removed the silent-return paths in the per-platform RTOS
E2E bodies (per CLAUDE.md's "tests must fail on unmet preconditions").
Running `cargo nextest run --test rtos_e2e` on a dev box with the full
SDK chain now unmasks two pre-existing production bugs that the old
silent-skip / lenient-assertion patterns were hiding. These are
distinct from the test-plumbing work in Groups A–C and belong in
their own phases, but are tracked here because 85.4 is what caused
them to stop silently passing.

- [~] 85.9 — Replace `nros-cpp` manual storage-size calc with
      compile-time derivation — **superseded by
      [Phase 87](./phase-87-nros-cpp-compile-time-sizes.md)**
  - **Files**: `packages/core/nros-cpp/build.rs`,
    `packages/core/nros-cpp/src/lib.rs`,
    `packages/core/nros-cpp/include/nros/nros_cpp_config_generated.h`
    (currently emitted by build.rs), optionally a new probe sub-crate.
  - **Goal**: Drop the hand-coded `4 * ptr_bytes + name_buf + …`
    formulas for `CPP_PUBLISHER_STORAGE_BYTES`,
    `CPP_SUBSCRIPTION_STORAGE_BYTES`,
    `CPP_SERVICE_STORAGE_BYTES`, and the action-server / action-client
    opaque sizes. The estimates currently under-count on armv7a
    (32-bit pointers + nested zenoh-pico handle state), causing
    `evaluation panicked: NROS_CPP_PUBLISHER_STORAGE_SIZE too small
    for CppPublisher` at the `lib.rs:350` compile-time assert when
    building NuttX / FreeRTOS C / C++ examples.
  - **Constraint**: The C++ classes (`Publisher`, `Subscription`,
    `ServiceServer`, `ServiceClient`, `ActionServer`, `ActionClient`)
    embed `alignas(8) uint8_t storage_[NROS_CPP_*_STORAGE_SIZE]`,
    which forces the size to be a C++ compile-time constant. build.rs
    runs on the *host*, so `size_of::<CppPublisher>()` inside build.rs
    gives the host pointer width, not the target's.
  - **Options** (pick one in the phase):
    1. **Probe crate**: new tiny crate `nros-cpp-sizes` that mirrors
       only the types whose sizes matter, compiled for the target
       during build.rs via `rustc --emit=obj` + `nm` symbol
       extraction, or nightly `-Zprint-type-sizes` parsing. Target-
       aware, no API change. Adds a build-time `rustc` invocation.
    2. **Heap-allocated opaque**: refactor the C++ classes to hold
       `void*`, let Rust `malloc` / deallocate exact-sized storage.
       Removes the whole class of undercount bugs and the build.rs
       math; cost is one extra allocation per handle and an API
       break for anyone reaching into `storage_` directly.
    3. **cbindgen-evaluated `const`**: define
       `pub const CPP_PUBLISHER_STORAGE_BYTES: usize =
        core::mem::size_of::<CppPublisher>();` in `nros-cpp/src/lib.rs`
       and check whether cbindgen evaluates the expression when
       emitting `nros_cpp_ffi.h`. If it does, this is the cleanest
       answer. If it doesn't (most likely — cbindgen parses AST, it
       doesn't run const-eval), fall back to 1 or 2.
  - **Blocks**: `test_rtos_{pubsub,service,action}_e2e::platform_2_Platform__Nuttx::lang_{2_Lang__C,3_Lang__Cpp}`
    (3 cases). Also blocks the FreeRTOS C / C++ equivalents once
    FreeRTOS E2E infrastructure is reliable (see 85.10 adjacent).
  - **Out of 85.4 scope**: a numeric bump (`handle_upper = 32 * ptr_bytes`)
    works locally but violates this phase's "no manual size calc"
    directive, so 85.4 left the original estimate in place and opens
    this phase to fix it properly.
  - **Status**: superseded by
    [Phase 87 — nros-cpp compile-time storage-size derivation](./phase-87-nros-cpp-compile-time-sizes.md),
    which picks **Option C** (shared types crate + probe crate) from
    the three alternatives listed above. The scope expanded beyond a
    single 85.x work item — it now includes a types-only crate
    refactor and a parallel fix for the same latent bug in `nros-c`.

- [x] 85.10 — Fix ThreadX QEMU RISC-V zenoh session connect failure
  - **Files**: `packages/core/nros-platform-threadx/src/net.rs`
    (NetX BSD socket shim — suspect after bisect),
    `packages/zpico/zpico-sys/zenoh-pico/src/transport/common/tx.c`
    and `src/link/link.c` (`_z_link_send_t_msg` →
    `_z_link_send_wbuf`), `packages/drivers/virtio-net-netx/`,
    possibly `packages/zpico/zpico-sys/` codegen for
    `NET_SOCKET_SIZE` ABI layout.
  - **Goal**: The RV64 firmware boots cleanly (virtio init, NetX IP
    stack up, BSD sockets initialised) but `Session::open(...)` fails
    with `Transport(ConnectionFailed)`. The slirp gateway at
    `10.0.2.2:7453` is reachable from the guest in principle —
    `QemuProcess::start_riscv64_virt` uses the same slirp setup as
    the other QEMU platforms that work.
  - **Reproducer**:
    ```
    cargo nextest run --test rtos_e2e -p nros-tests \
      -E 'test(rtos_pubsub_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust)'
    ```
    Failure message: `Application error: Transport(ConnectionFailed)`.
  - **Scope**:
    - Verify zenohd is reachable from the RV64 slirp guest (tcpdump
      on host loopback during the test).
    - Check whether the failure is at TCP SYN (network routing
      issue), at the zenoh handshake (protocol mismatch / transport
      layer bug), or inside zenoh-pico's session negotiation.
    - The existing ThreadX Linux port table entry (bridge-networked)
      works; the RV64 port uses slirp. Compare the two paths.
  - **Bisect so far (85.10a, 2026-04-21)** — instrumented
    `Context::with_config` (zpico.rs), zenoh-pico `zpico_open` (C),
    and the Rust-side `tcp_open` / `tcp_send` in
    `nros-platform-threadx/src/net.rs` with UART-print probes. Boot
    sequence:
    1. `nx_bsd_socket()` — OK, valid fd.
    2. `nx_bsd_connect(10.0.2.2:7453)` — returns `0` (success).
       Zenohd-side `tcpdump` not run due to loopback-BPF permissions,
       but the stub-socket success means NetX's TCP stack reached the
       slirp gateway.
    3. Exactly **one** `_z_send_tcp` call with `len == 0`, then
       zenoh-pico `z_open()` returns `_Z_ERR_TRANSPORT_TX_FAILED`
       (-100). No `_z_read_tcp` / `_z_read_exact_tcp` ever called.
    4. The fact that the *only* `_z_send_tcp` call has `len == 0`
       strongly suggests the failure is **before** the INIT(Syn)
       message actually gets encoded / emitted — most likely inside
       `_z_link_send_t_msg` (`transport/common/tx.c:417`) or the
       wbuf-iosli iteration in `_z_link_send_wbuf`
       (`link/link.c:205`). Either the wbuf is empty (encoding
       failed silently) or a single empty iosli is iterated.
    5. Debug probes were reverted after the bisect — the next
       investigator should re-apply them locally and check two
       hypotheses: (a) `zl->_cap._flow` is not
       `Z_LINK_CAP_FLOW_STREAM` for the RV64 ThreadX TCP link (which
       skips the length-prefix write and produces an empty wbuf),
       (b) `_z_wbuf_make(mtu, false)` is returning a wbuf with zero
       capacity due to a `z_malloc` failure somewhere upstream.
    6. Also worth comparing `NET_SOCKET_SIZE` / `NET_ENDPOINT_SIZE`
       between host and target (see `zpico-platform-shim`'s
       `platform_net_sizes.rs`) — a cross-size mismatch would
       corrupt the ABI when `ZSysNetSocket` is passed by value to
       `_z_send_tcp`.
  - **Cross-reference**: my session memory notes a related
    `ThreadX Linux x86_64 pointer truncation` investigation
    (`packages/drivers/nsos-netx`) — the RV64 driver
    (`virtio-net-netx`) may have a similar class of pointer-size /
    socket-handle bug.
  - **Blocks**: 7 `rtos_e2e` cases
    (`platform_4_Platform__ThreadxRiscv64 × {Rust, C, Cpp} ×
     {pubsub, service, action}` minus the 2 cases skipped by
     `skip_reason` for missing C++ service / action examples).
  - **Root cause found (85.10b, 2026-04-21)** — hypothesis (c) (ABI
    size mismatch) confirmed. The size probe in
    `zpico-platform-shim/build.rs` used `env::var("FREERTOS_DIR").is_ok()`
    as its first branch, but `.envrc` exports `FREERTOS_DIR`
    globally. Every cross-compile — including RV64 ThreadX — took
    the FREERTOS branch, failed to compile the probe (couldn't find
    lwIP headers), fell back to hardcoded `SOCKET_SIZE=16 /
    ENDPOINT_SIZE=8`, and emitted a Rust shim whose opaque wrapper
    for `_z_sys_net_socket_t` was 12 bytes too large. Pass-by-value
    marshaling of `sock` across the `_z_send_tcp(sock, ptr, len)`
    FFI boundary shifted the arguments one register down — Rust
    read whatever was in a3 as `len`, which happens to be zero at
    function entry, hence the "tcp_send len=0x00000000" probe trace.
    Fix: switch the probe's branch key from "SDK env var set" to
    "target triple + SDK env var set", mirror the full ThreadX /
    NetX / picolibc include chain from the main build (otherwise
    the probe still fails to compile for RV64), and turn the silent
    fallback into a loud `cargo:warning`. Committed as `7d79276e`.
  - **Result**:
    `cargo nextest run -p nros-tests --test rtos_e2e
     -E 'test(rtos_pubsub_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust)'`
    passes in 73 s (previously failed at 20 s with
    `Transport(ConnectionFailed)`).

## Acceptance Criteria

- [~] `just test` wall time reduced ≥15 % — **carried to Phase 89**.
      Baseline not captured before 85.1 landed (phase doc's own
      self-caveat); post-85 count drifted back up as new phases
      added coverage, so a bare-number comparison is no longer
      meaningful. Subjective: `just test` is materially faster than
      pre-85 because of 85.2 (sleep → ready-probe) and 85.3 (64
      build-only tests deleted).
- [~] `just test-all` wall time reduced ≥30 % — **carried to Phase 89**
      for the same reasons as above.
- [~] Total integration test function count <100 — **not met; carried
      to Phase 89**. Count is 260 as of 2026-04-24 (vs. 214 baseline
      / <100 target). Phase 85 did achieve the *mechanical*
      consolidation (60+ build-only tests gone, 32-body RTOS E2E
      matrix collapsed to one parametrised file), but phases 84 / 86
      / 87 outran the reduction by adding new coverage. Further
      reduction requires different techniques (deduplication across
      transports, parallelisation) tracked by Phase 89.
- [x] No platform coverage lost: every (platform, language, variant)
      combination currently tested still has at least one passing
      assertion in the new suite. (Verified by 85.4 + 85.8.)
- [~] `rg 'sleep\(Duration::from_secs\([2-9]'` returns zero results
      outside of intentional pacing in long-lived E2E fixtures —
      **mostly met; three stragglers carried to Phase 89**:
      `tests/ros2_lifecycle_interop.rs:75` (2 s),
      `tests/zero_copy.rs:85` and `:145` (3 s each). The fixture
      sleep in `src/fixtures/zenohd_router.rs:152` and the doc-comment
      sleep in `src/process.rs:123` are both exempt.
- [x] Single source of truth: adding a new RTOS platform is a one-line
      entry in `nros_tests::platform` + one `start_X_virt` fixture
      helper, not a new test `.rs` file. (85.4 + 85.5.)

## Notes & Caveats

- **Don't break the Phase 77 / Phase 83 regression coverage.** The
  `test_cpp_action_communication`, `test_cpp_action_goal_rejection`,
  `test_nuttx_c_{pubsub,service,action}_e2e` tests are the load-bearing
  regression guards for recently-landed phases. Each must still be a
  discrete, nameable assertion — parametrisation is fine, outright
  deletion isn't.
- **Don't rely on "the E2E test covers the build path" for
  toolchain-gate tests.** The NuttX C++ `test_X_builds` functions are
  `#[ignore]`d as a declarative marker for the upstream
  `_SC_HOST_NAME_MAX` issue; the E2E test wouldn't run either way.
  Keep those markers.
- **Don't over-parametrise.** A `#[rstest]` matrix that spans 4
  platforms × 3 languages × 3 variants = 36 test cases is fine, but if
  a single case has per-platform quirks (like NuttX's C service test
  needing `#[ignore]`), split the matrix rather than hiding the
  `if platform == NUTTX { ... }` branch inside the test body.
- **Baseline the timings before 85.1 lands.** Capture the current
  `just test` / `just test-all` wall time on reference hardware;
  the acceptance criteria above are percentages, not absolute targets,
  and the percentages need a baseline to check against.

## How to run the audit again later

```
# Enumerate test functions per file
rg -n '^(pub )?fn test_' packages/testing/nros-tests/tests/ | wc -l

# Find slow sleeps
rg -n 'sleep\(Duration::from_secs\([2-9]' packages/testing/nros-tests/

# Spot per-file OnceCell caches (drift indicator)
rg -n 'static [A-Z_]+_BINARY: OnceCell<PathBuf>' packages/testing/nros-tests/
```

Re-run after every phase closure: each phase that adds a new platform
or example should leave these counts stable or smaller, never larger.
