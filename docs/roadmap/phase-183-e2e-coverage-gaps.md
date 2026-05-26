# Phase 183 - E2E test coverage gaps (platform × lang × rmw × case + ROS 2 interop)

**Goal.** Close the runtime E2E coverage holes found by the 2026-05-26 audit:
every `examples/` cell that *exists* (per `examples/README.md`) should have a
runtime E2E test exercising its pub/sub, service, and action cases, and each RMW
backend should have ROS 2 interop coverage proportional to its purpose. Fill the
gaps; do **not** invent tests for intentionally-empty cells.

**Status.** In progress. Created 2026-05-26 from the E2E completeness audit (run
against the `cargo nextest list` inventory + the `examples/README.md` coverage
matrix, after Phase 182's test de-dup). **183.5 landed** — the CycloneDDS↔ROS 2
interop test scaffolding (detection passes; interop cases `#[ignore]`d pending
the 117.12 product work). 183.1–183.4, 183.6 open.

**Priority.** P2 (test coverage / regression confidence). The CycloneDDS↔ROS 2
item (183.4) is P1-adjacent — it is Phase 117's core goal and currently has
**zero** test coverage.

**Depends on / Related.**
- **`examples/README.md`** coverage matrix — authoritative for which cells exist
  vs are intentionally empty. A gap here means "example exists, no E2E"; do not
  add E2E for an intentionally-empty cell.
- **Phase 182** — the test de-dup that motivated this audit (and dropped the
  build-only smokes, leaving E2E as the real coverage signal).
- **Phase 177.30** — NuttX Cpp action lease-task hang; gates the nuttx action
  E2E re-enable (183.1).
- **Phase 117 / 117.12** — stock `rmw_cyclonedds_cpp` interop; 183.4 is its test.
- **Phase 171.C.3 / 177.22** — pending CycloneDDS service/action on
  threadx-linux / threadx-riscv64; bounds 183.3.

## Coverage matrix (2026-05-26)

Cases: pubsub / service / action. ✓ = E2E exists. `—` = no example (out of
scope). **✗** = example exists but **no E2E** (a gap). "drop"/"pend" = tracked
elsewhere (not a blind-fill target).

### Zenoh
| platform | langs | pubsub | service | action |
|----------|-------|:---:|:---:|:---:|
| native | c/cpp/rust | ✓ | ✓ | ✓ |
| qemu-arm-baremetal | rust | ✓ | ✓ | ✓ |
| freertos | c/cpp/rust | ✓ | ✓ | ✓ |
| nuttx | c/cpp/rust | ✓ | ✓ | drop (177.30) |
| threadx-linux | c/cpp/rust | ✓ | ✓ | ✓ |
| threadx-riscv64 | c/cpp/rust | ✓ | ✓ | drop (182.5) |
| esp32 / qemu-esp32 | rust | ✓ | — | — |
| stm32f4 | rust | — (no QEMU; cross-build only) | | |
| zephyr | cpp | ✓ | ✓ | ✓ |
| zephyr | rust | ✓ | **✗** | ✓ |
| zephyr | c | **✗** | **✗** | **✗** |

### XRCE (examples exist only on native + zephyr)
| platform | lang | pubsub | service | action |
|----------|------|:---:|:---:|:---:|
| native | rust | ✓ | ✓ | ✓ |
| native | c | ✓ | **✗** | **✗** |
| zephyr | rust | ✓ | ✓ | ✓ |
| zephyr | cpp | ✓ | ✓ | ✓ |
| zephyr | c | ✓ | **✗** | **✗** |

### CycloneDDS
| platform | langs | pubsub | service | action |
|----------|-------|:---:|:---:|:---:|
| zephyr | c/cpp/rust | ✓ | ✓ | ✓ |
| native | c/cpp(+rust) | ✓ | **✗** | **✗** |
| freertos | rust | ✓ (local boot) | pend | pend |
| threadx-linux | →native | ✓ | pend (171.C.3) | pend |
| threadx-riscv64 | c | ✓ (two-QEMU, gated) | pend (177.22) | pend |

### ROS 2 interop (nano-ros ↔ stock ROS 2)
| backend | pubsub | service | action | extras |
|---------|:---:|:---:|:---:|--------|
| zenoh (`rmw_interop`) | ✓ 2-way | ✓ 2-way | ✓ 2-way | discovery, qos, latency, throughput — complete |
| xrce (`xrce_ros2_interop`) | ✓ 2-way | ✓ 1-way only | **✗** | dds_detection |
| cyclonedds | **✗** | **✗** | **✗** | none |
| lifecycle | `ros2_lifecycle_full_cycle` | | | |

## Work Items

### 183.1 — Zephyr C zenoh + xrce E2E (largest hole)

`examples/zephyr/c/` ships 6 zenoh + 6 xrce cases but the only C E2E is the xrce
talker/listener boot. The Rust + C++ zephyr E2E suites (in `tests/zephyr.rs` +
`tests/phase_118_collapse.rs`) are the template — add the C analogues:
- zenoh C: talker→listener pubsub, service roundtrip, action goal/feedback/result.
- xrce C: service + action (pubsub already covered by `test_zephyr_xrce_c_talker_listener`).
Use `build_zephyr_cmake_example_rmw("c", …)` (already used by the cyclonedds C
e2e) + `ZephyrProcess`. **Files**: `tests/zephyr.rs`. **Est.**: ~6 tests.

**Attempt 2026-05-26 — BLOCKED on the C fixtures, not the tests.** Drafted the 5
tests (zenoh pubsub/service/action + xrce service/action) mirroring the passing
cyclonedds C e2e — same resolver (`build_zephyr_cmake_example_rmw("c", …)`,
build root `build/zephyr-workspace-builds/`), same `ZephyrProcess` harness, C
client success markers `Result:` (service) / `Result status:` (action). They
compile clean but **all 5 fail at runtime** because the C zenoh/xrce fixtures in
that build root don't reach the app:
- **zenoh C (all 3):** the node logs `zpico_zephyr: Network not ready after
  2000 ms` → `nros_listener: Network not ready` and exits before publishing.
  The C and Cpp zenoh `prj.conf` + `prj-zenoh.conf` net configs are **identical**
  (both NSOS, neither has `NET_CONNECTION_MANAGER`), and the C *cyclonedds*
  fixtures in the **same** build root pass — so it's specific to zenoh-pico's
  `zpico_zephyr_wait_network` timing out on native_sim in this build, not the
  test or the net Kconfig.
- **xrce C service/action:** client gets no `Result:`/`Result status:`. The
  *existing* passing xrce-C pubsub e2e (`test_zephyr_xrce_c_talker_listener`)
  resolves via `get_zephyr_xrce_c_*_native_sim` → the **`nano-ros-workspace/`**
  in-place west build (build system #1), a *different* fixture set than the
  `build/zephyr-workspace-builds/` SSOT root (#2) the new tests + cyclone e2e
  use. So the two build systems are not yet aligned for zenoh/xrce.

**Root issue to resolve first (then the tests pass as-is):** the Phase-181 SSOT
build root (`build/zephyr-workspace-builds/`) produces working *cyclonedds* C
fixtures but zenoh/xrce C fixtures that fail native_sim network-readiness, while
the legacy `nano-ros-workspace/` builds (used by the surviving zenoh/xrce e2e,
now flagged stale by the staleness gate) worked. Align the two: either fix the
SSOT zenoh/xrce native_sim build (why `wait_network` times out there) or point
the C e2e at the build system that the Rust/Cpp zenoh/xrce e2e use. The test
code itself is proven (cyclone C e2e use the identical pattern and pass);
reverted pending the fixture fix to avoid red tests in `test-all`.

### 183.2 — Native + Zephyr XRCE C service/action E2E

C XRCE examples exist (native 6, zephyr 6) but only pubsub is exercised.
- native C: `tests/c_xrce_api.rs` has `test_c_xrce_talker_listener_communication`
  (pubsub) — add service request/response + action goal e2e against an
  `XrceAgent` (mirror the Rust `tests/xrce.rs` service/action tests).
- zephyr C: covered by 183.1's xrce-C service/action.
**Files**: `tests/c_xrce_api.rs`, `tests/zephyr.rs`. **Est.**: ~3 tests.

### 183.3 — Zephyr Rust zenoh service E2E

`tests/zephyr.rs` has rust zenoh pubsub + action e2e but no service; the cpp
sibling (`test_zephyr_cpp_service_server_to_client_e2e`) is the template. Add
`test_zephyr_rust_service_e2e` (zenoh). **Files**: `tests/zephyr.rs`. **Est.**: 1.

### 183.4 — Native CycloneDDS service + action E2E

`examples/native/{c,cpp}/` cyclonedds ships 6 cases each but `tests/native_api.rs`
only e2e-tests pubsub (`test_native_cyclonedds_*_talker_to_listener`). Add
service + action e2e for the native Cyclone path (CMake/Corrosion fixtures,
Phase 175). The Cyclone C++ action get_result/feedback path landed in
`28e9e6502` + the Phase 171.0.b follow-ups — this test pins it. **Files**:
`tests/native_api.rs`. **Est.**: ~4 tests (service + action × {c, cpp}).

### 183.5 — CycloneDDS ↔ ROS 2 interop — DONE (scaffolding; interop pending 117.12)

**Landed:** new `tests/cyclonedds_ros2_interop.rs` mirroring `rmw_interop.rs` /
`xrce_ros2_interop.rs` — a nano-ros Cyclone node + a stock `rmw_cyclonedds_cpp`
ROS 2 node on a shared `ROS_DOMAIN_ID`:
- `test_cyclonedds_ros2_detection` (always runs; reports ROS 2 + rmw_cyclonedds
  availability — verified PASS locally, both present).
- `test_cyclonedds_nano_to_ros2_pubsub`, `test_cyclonedds_ros2_to_nano_pubsub`,
  `test_cyclonedds_service_nano_server_ros2_client` — `#[ignore]`d with a 117.12
  reason (stock Cyclone wire interop not passing yet), so they exist as tracked,
  runnable coverage (`--run-ignored all`) rather than a silent gap, and flip to
  passing as 117.X lands. Each skips cleanly when ROS 2/`rmw_cyclonedds_cpp` or
  the native Cyclone fixtures are absent.

Infra added to `src/ros2.rs`: `is_rmw_cyclonedds_available` / `require_ros2_cyclonedds`,
`ros2_env_setup_rmw_with_domain` (RMW-parametrized; the fastrtps `_dds_` setup now
delegates to it) + `ros2_env_setup_cyclonedds_with_domain`, and three
`Ros2DdsProcess::*_cyclonedds_with_domain` constructors (topic echo/pub, service
call). `.config/nextest.toml` gets a `cyclonedds_ros2_interop` group (max-threads
3, per-test distinct domains) + `retries = 2`.

Verified: compiles clean, detection passes, the interop harness reaches a clean
`skip!` when the Cyclone C fixtures aren't prebuilt. Remaining (the product side,
**not this phase**): make the interop actually pass — Phase 117.12 / 117.X stock
`rmw_cyclonedds_cpp` wire-compat. Drop each `#[ignore]` as its case starts
working (run after `just cyclonedds setup` + `just build-test-fixtures`).

#### original plan

**Highest-value gap.** CycloneDDS exists to be wire-compatible with stock
`rmw_cyclonedds_cpp`, yet no test exercises nano-ros↔ROS 2 over Cyclone. Stand up
a `tests/cyclonedds_ros2_interop.rs` mirroring `tests/rmw_interop.rs`
(zenoh↔ROS 2): nano-pub → ROS 2 sub and ROS 2 pub → nano-sub on a shared
`ROS_DOMAIN_ID`, then service both directions. Gated on a ROS 2 +
`rmw_cyclonedds_cpp` environment (skip cleanly when absent, same as
`rmw_interop`). Tracks Phase 117.12; surfaces the known stock-interop failures as
explicit `#[ignore]`d-or-failing tests rather than silent absence. **Files**:
new `tests/cyclonedds_ros2_interop.rs`, `.config/nextest.toml` (group + gate).
**Est.**: ~6 tests. **Depends on**: 117.X service-envelope / topic-prefix work.

### 183.6 — XRCE ↔ ROS 2: action + reverse-direction service

`tests/xrce_ros2_interop.rs` covers pubsub both ways + service (xrce-server /
ros2-client only). Add:
- action: nano-XRCE server ↔ ROS 2 action client (and reverse).
- service: ROS 2 server ↔ nano-XRCE client (the missing direction).
Mirror the `rmw_interop` zenoh action/service-both-ways shape, gated on the
Micro XRCE-DDS Agent + ROS 2 DDS. **Files**: `tests/xrce_ros2_interop.rs`.
**Est.**: ~3 tests.

## Not in scope (tracked elsewhere — do not blind-fill)

- **nuttx + threadx-riscv64 zenoh action** — examples exist, E2E deliberately
  dropped (Phase 182.5; NuttX is 177.30's lease-task hang). Re-enable when
  177.30 lands, not here.
- **freertos / threadx-linux / threadx-riscv64 CycloneDDS service+action** —
  fixtures partly build-only; runtime blocked on Phase 171.C.3 / 177.22. Fill
  after those land.
- **intentionally-empty cells** (`examples/README.md`): bare-metal C/C++,
  esp32/stm32f4 C/C++, px4 C/Rust, Cyclone on bare-metal/NuttX, pure-cargo
  Cyclone Rust. No examples → no E2E expected.

## Acceptance

- [ ] Every non-empty `examples/README.md` cell with a service/action case has a
  matching runtime E2E test (or a tracked-elsewhere exemption noted above).
- [ ] CycloneDDS has ROS 2 interop coverage (183.5), even if some cases start
  `#[ignore]`d pending 117.X.
- [ ] XRCE↔ROS 2 covers action + both service directions (183.6).
- [ ] New tests follow the suite conventions: `nros_tests::skip!` on unmet
  preconditions (never silent early-return), per-platform nextest groups +
  `retries` for process-heavy E2E (Phase 177/G6), readiness waits not fixed
  sleeps.
- [ ] `examples/README.md` coverage matrix and the E2E suite agree (a follow-up
  audit reproduces a clean matrix).

## Notes

- This phase adds **tests only** — it does not add examples (the examples already
  exist for every gap cell) and does not fix product bugs (those that surface,
  e.g. stock-Cyclone interop, route to Phase 117/177).
- Audit method: `cargo nextest list` cross-referenced against
  `examples/README.md` — re-run both after each item to confirm the gap closed.
- Sequence by value: 183.5 (Cyclone↔ROS 2, the core-goal blind spot) and 183.1
  (zephyr C, the biggest example-vs-test hole) first; 183.2/183.3/183.4/183.6 are
  smaller fills.
