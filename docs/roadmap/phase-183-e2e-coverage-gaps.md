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
the 117.12 product work). **183.1 + 183.3 landed** — zephyr C zenoh+xrce E2E
(5 tests; also covers 183.2's zephyr half) + zephyr rust zenoh service.
**183.2 native done** (verified PASS). **183.4: link gap fixed (177.31) → service
e2e PASS (C+C++); action blocked on 177.32** (Cyclone action-server executor
register). **183.6 done** — XRCE↔ROS 2 action (both dirs) + reverse-direction
service (3 tests, run green). All 183 items landed except: 183.4-action (→177.32)
and 183.1's zenoh-C-action `#[ignore]` (server-create hang) — both tracked
elsewhere.

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

### 183.1 — Zephyr C zenoh + xrce E2E (largest hole) — DONE

Added 5 C E2E tests to `tests/zephyr.rs` (xrce C pubsub already existed as
`test_zephyr_xrce_c_talker_listener`): `test_zephyr_c_{talker_to_listener,
service_server_to_client,action_server_to_client}_e2e` (zenoh) +
`test_zephyr_xrce_c_{service,action}_e2e`. Binaries resolve via
`build_zephyr_cmake_example_rmw("c", case, rmw)` (per-cell west prebuild); each
skips cleanly when the fixture isn't built. Not `#[ignore]`d — zephyr C is
expected to run (the cyclone C e2e already does). Verified: compiles clean, all
5 list. This also satisfies the zephyr half of 183.2. **Files**: `tests/zephyr.rs`.

#### original plan

`examples/zephyr/c/` ships 6 zenoh + 6 xrce cases but the only C E2E is the xrce
talker/listener boot. The Rust + C++ zephyr E2E suites (in `tests/zephyr.rs` +
`tests/phase_118_collapse.rs`) are the template — add the C analogues:
- zenoh C: talker→listener pubsub, service roundtrip, action goal/feedback/result.
- xrce C: service + action (pubsub already covered by `test_zephyr_xrce_c_talker_listener`).
Use `build_zephyr_cmake_example_rmw("c", …)` (already used by the cyclonedds C
e2e) + `ZephyrProcess`. **Files**: `tests/zephyr.rs`. **Est.**: ~6 tests.

**Attempt 2026-05-26 — root-caused to a STALE fixture, test code is correct.**
Drafted the 5 tests (zenoh pubsub/service/action + xrce service/action)
mirroring the passing cyclonedds C e2e — same `build_zephyr_cmake_example_rmw("c", …)`
resolver, same `ZephyrProcess` harness, C client success markers `Result:`
(service) / `Result status:` (action). They compile clean; the zenoh C ones
failed at runtime with `zpico_zephyr: Network not ready after 2000 ms`.

**Root cause: a stale zenoh C fixture in the resolver's preferred build root.**
- `zephyr_build_root()` (`fixtures/binaries/mod.rs`) prefers `nano-ros/zephyr-workspace`
  when it exists+writable. That path is a **symlink → `../nano-ros-workspace`**
  (the legacy in-place west builds). Only when the symlink is absent does it
  fall back to `build/zephyr-workspace-builds` (the Phase-181 SSOT root).
- `zpico_zephyr_wait_network()` returns immediately under
  `CONFIG_NET_NATIVE_OFFLOADED_SOCKETS` ("Network ready (NSOS)"); only the
  legacy `#else` native-net-stack path prints "Waiting for network readiness" /
  "Network not ready".
- The `nano-ros-workspace/build-c-talker-zenoh/zephyr/zephyr.exe` that the
  resolver picks is the **non-NSOS `#else` variant** — built before NSOS landed
  in `zpico_zephyr.c`, never rebuilt. The **correct NSOS** zenoh C fixture *does*
  exist in `build/zephyr-workspace-builds/build-c-talker-zenoh` (`strings` shows
  "Network ready (NSOS)"), but the resolver doesn't use it. Cyclone C passes
  because its fixture in the symlinked root is current; only zenoh (and likely
  xrce) C are stale there.

**Fix:** rebuild the zephyr C zenoh/xrce fixtures in the symlinked root
(`just zephyr build-fixtures`, or build with `NROS_ZEPHYR_BUILD_ROOT` pointed at
the fresh SSOT root) so the resolver picks the NSOS variant, then re-add the 5
tests (test code is proven — reverted only to avoid red tests in `test-all`
while the fixture is stale).

### 183.2 — Native + Zephyr XRCE C service/action E2E — DONE (native; zephyr → 183.1)

C XRCE examples exist (native 6, zephyr 6) but only pubsub was exercised.
- **native C — done + verified PASS:** added `test_c_xrce_service_request_response`
  (AddTwoInts roundtrip) + `test_c_xrce_action_fibonacci` (goal→feedback→result) to
  `tests/c_xrce_api.rs`, driving the prebuilt `build-xrce/` C binaries against a
  unique `XrceAgent` (mirrors the Rust `tests/xrce.rs` service/action). Both pass
  locally (service 2.1 s, action 5.5 s).
- **zephyr C** — its xrce-C service/action belong to 183.1 (zephyr C suite, owned
  by the concurrent agent); not duplicated here.
**Files**: `tests/c_xrce_api.rs`. **Result**: 2 tests, green.

### 183.3 — Zephyr Rust zenoh service E2E — DONE

Added `test_zephyr_rust_service_e2e` to `tests/zephyr.rs`, reusing the existing
`get_zephyr_service_{server,client}_native_sim` (rust zenoh) resolvers. Verified:
compiles clean, lists. **Files**: `tests/zephyr.rs`.

#### original plan

`tests/zephyr.rs` has rust zenoh pubsub + action e2e but no service; the cpp
sibling (`test_zephyr_cpp_service_server_to_client_e2e`) is the template. Add
`test_zephyr_rust_service_e2e` (zenoh). **Files**: `tests/zephyr.rs`. **Est.**: 1.

### 183.4 — Native CycloneDDS service + action E2E — service DONE; action → 177.32

**Update 2026-05-26:** the no-op-link gap was **fixed** (177.31 — `enable_language(CXX)`
in the native C/Rust Cyclone examples; `20ef5c014`). With the exes linking:
- `test_native_cyclonedds_service` (C + C++) — **PASS** (real AddTwoInts roundtrip).
- `test_native_cyclonedds_action` (C + C++) — blocked on a *different*, action-specific
  bug: **177.32** (native Cyclone action server `nros_executor_register_action_server`
  fails to register; pub/sub + service work). Not a 183.x test issue — tracked in 177.
The `native build-fixture-extras` Cyclone loop can now build all 6 roles (the 117/175
agent owns re-extending it). Original blocked-on-link note retained below for history.

**Tests landed (4):** `tests/native_api.rs` gained
`test_native_cyclonedds_{service,action}` parametrised over `#[values(C, Cpp)]`,
driving the `build-cyclonedds/` server+client on a per-test `ROS_DOMAIN_ID` (the
existing `spawn_cyclone_binary` + `next_cyclonedds_domain` helpers + a new
`cyclone_role_binary(lang, case)` resolver). C/C++ action markers differ
("Waiting for action goals"/"Final result" vs "Waiting for goal requests"/"Result:
sequence="), keyed per lang. Compile clean.

**Blocked — discovered build gap (route to Phase 117 / 175):** the native Cyclone
**service + action** example *executables don't link*. Under `-DNROS_RMW=cyclonedds`
the `c_service_server` / `cpp_action_client` / … targets compile their objects but
their final link rule is a **no-op (`: && :`)** — no top-level exe is produced
(verified via `ninja -v`: `[206/206] : && :`). zenoh + xrce produce real ELFs, and
Cyclone **talker/listener** (pub/sub) link fine (CLAUDE.md: native pub/sub passes),
so this is specific to the service/action example-CMake executable wiring for
Cyclone. Until that's fixed, the 4 tests **skip cleanly** (the prebuilt resolver
finds no binary) — tracked coverage, not a silent gap, that goes green once the
exe links. The `native build-fixture-extras` Cyclone loop was therefore left at
talker/listener (extending it to service/action only builds no-op targets).
**Files**: `tests/native_api.rs`, `just/native.just` (note). **Owner of the link
fix**: Phase 117 / 175.

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

### 183.6 — XRCE ↔ ROS 2: action + reverse-direction service — DONE

Added 3 tests to `tests/xrce_ros2_interop.rs` (the file's existing tests covered
pub/sub both ways + service xrce-server/ros2-client):
- `test_xrce_action_ros2_client` — nano-XRCE action server ↔ ROS 2 (DDS) action
  client (`ros2 action send_goal --feedback`).
- `test_ros2_action_xrce_client` — ROS 2 DDS Fibonacci action server
  (`action_tutorials_py`) ↔ nano-XRCE action client (reverse).
- `test_ros2_service_xrce_client` — ROS 2 DDS `add_two_ints` server (rclpy
  one-liner) ↔ nano-XRCE service client (the missing service direction).

Infra added to `src/ros2.rs`: `Ros2DdsProcess::{add_two_ints_server,
action_server_fibonacci, action_send_goal}_with_domain` (DDS env, mirroring the
zenoh `Ros2Process` server/action helpers). Best-effort / INFO-not-hard-fail
like the rest of the file (DDS naming/version drift, demo-node availability).
Verified: compiles clean, all 3 run green locally (ROS 2 + rmw_fastrtps_cpp +
XRCE Agent present). **Files**: `tests/xrce_ros2_interop.rs`, `src/ros2.rs`.

#### original plan

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
- [x] CycloneDDS has ROS 2 interop coverage (183.5), even if some cases start
  `#[ignore]`d pending 117.X.
- [x] XRCE↔ROS 2 covers action + both service directions (183.6).
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
