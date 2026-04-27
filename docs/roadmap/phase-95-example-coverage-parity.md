# Phase 95 — Example Coverage Parity (Zephyr + Native DDS)

**Goal**: Close the `(platform × lang × backend × use-case)` example
matrix on platforms where the underlying RMW backend already works.
Today only the `(native, rust, zenoh)` cell is fully populated;
`(zephyr, *, *)` and `(native, *, dds)` are missing service / action /
C / C++ entries even though `nros-c`, `nros-cpp`, `nros-rmw-dds`, and
`nros-rmw-xrce` all support those use-cases at the API level.

**Status**: Not Started

**Priority**: Medium. Examples are the primary onboarding surface — a
user copying out a Zephyr xrce example for a service node hits a wall
because none ships.

**Depends on**: Phase 71.8 (Zephyr DDS pubsub on `native_sim` / `cortex_a9`
— `[x]` for cortex_a9 via Phase 92, `[~]` for native_sim), Phase 86
(`nros-lifecycle-msgs`), Phase 87 (cpp compile-time sizes).

## Overview

### Current coverage matrix

| Tree | Backend | talker | listener | svc-srv | svc-cli | act-srv | act-cli | async-svc | lifecycle |
|------|---------|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|
| zephyr/rust  | zenoh | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| zephyr/rust  | xrce  | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| zephyr/rust  | dds   | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | — |
| zephyr/cpp   | zenoh | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| zephyr/cpp   | xrce  | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | — | — |
| zephyr/cpp   | dds   | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | — | — |
| zephyr/c     | zenoh | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| zephyr/c     | xrce  | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| zephyr/c     | dds   | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | — | — |
| native/rust  | zenoh | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| native/rust  | xrce  | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| native/rust  | dds   | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| native/c     | zenoh | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| native/c     | xrce  | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| native/c     | dds   | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | — | — |
| native/cpp   | zenoh | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| native/cpp   | dds   | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | — | — |

Total missing example crates: **52** (Zephyr 27, native 25). All gated
on backends that already compile + run; no new RMW work required.

### Out of scope

* New RMW backends or platforms (Phase 71 covers DDS on
  freertos/nuttx/threadx/baremetal/esp32; Phase 90 covers `rmw-uorb`).
* `lifecycle-node` / `async-*` / `rtic-*` for backends other than
  `(native, rust, zenoh)` — those are RMW-edge experiments, not
  onboarding examples. Add only when an actual user asks.
* `custom-msg`, `fairness-bench`, `stress-test`, `large-msg-test` —
  benchmark / regression rigs, not coverage targets.

## Architecture / Design

### One canonical example per cell

Each new crate is a near-mechanical port of the matching `zenoh` cell:
swap RMW feature flag in `Cargo.toml` (`rmw-zenoh` → `rmw-xrce` /
`rmw-dds`), keep the topology + spin loop unchanged, regenerate
`generated/` via `cargo nano-ros generate-rust|c|cpp`. The standalone
project property (CLAUDE.md "Examples are Standalone Projects") means
every cell needs its own `Cargo.toml` / `CMakeLists.txt` / per-platform
support cmake — no cross-cell sharing.

### Test matrix structure

* One nextest E2E test per example crate. Pattern: lift the existing
  `test_native_zenoh_*` / `test_zephyr_zenoh_*` test, swap fixture
  binary names, swap `ZenohRouter::start()` for the matching
  `XrceAgent::start()` / DDS multicast fixture.
* Per-backend nextest groups already exist
  (`native-zenoh`, `native-xrce`, `qemu-zephyr`, `qemu-zephyr-dds`).
  New tests slot into the same groups — no new serialisation work.

### Build / fixture wiring

* Native: each new crate listed in `examples/native/{c,cpp,rust}/<rmw>/`
  is picked up by `just native build-fixtures` / `cargo build`
  workspace exclusion is per-crate, follow the existing pattern.
* Zephyr: each new crate needs a `just zephyr build-<rmw>-<lang>-<role>`
  recipe + `nros_tests::zephyr::ZephyrProcess::*` variant. Reuse
  `start_qemu_a9_mcast` for DDS, the existing `native_sim` launcher for
  xrce.

## Work Items

- [x] 95.A1 — Zephyr xrce-rust service-server
- [x] 95.A2 — Zephyr xrce-rust service-client
- [x] 95.A3 — Zephyr xrce-rust action-server
- [x] 95.A4 — Zephyr xrce-rust action-client
- [ ] 95.B1 — Zephyr dds-rust service-server
- [ ] 95.B2 — Zephyr dds-rust service-client
- [ ] 95.B3 — Zephyr dds-rust action-server
- [ ] 95.B4 — Zephyr dds-rust action-client
- [ ] 95.B5 — Zephyr dds-rust async-service-client
- [ ] 95.C1–6 — Zephyr cpp-xrce: talker, listener, svc-server, svc-client, action-server, action-client
- [ ] 95.D1–6 — Zephyr cpp-dds: talker, listener, svc-server, svc-client, action-server, action-client
- [ ] 95.E1–6 — Zephyr c-dds: talker, listener, svc-server, svc-client, action-server, action-client
- [ ] 95.F1 — Native dds-rust service-server
- [ ] 95.F2 — Native dds-rust service-client
- [ ] 95.F3 — Native dds-rust action-server
- [ ] 95.F4 — Native dds-rust action-client
- [ ] 95.G1–6 — Native c-dds: talker, listener, svc-server, svc-client, action-server, action-client
- [ ] 95.H1–6 — Native cpp-dds: talker, listener, svc-server, svc-client, action-server, action-client
- [ ] 95.I — `just test-all` integration (all new tests pass)
- [ ] 95.J — Coverage matrix verification (this doc's table flips to all-✅)

### 95.A — Zephyr xrce-rust completeness

Port four crates from `examples/zephyr/rust/zenoh/{service-server,
service-client,action-server,action-client}` to xrce. Each new crate:

* `Cargo.toml` features `rmw-xrce,platform-zephyr`.
* `prj.conf` includes `CONFIG_NROS_RMW_XRCE=y` (already present in
  existing xrce talker/listener).
* `nros_tests::zephyr::ZephyrProcess` variant for the new fixtures.
* nextest test starting `XrceAgent::start(platform::ZEPHYR.xrce_port)`.

**Files**:
- `examples/zephyr/rust/xrce/{service-server,service-client,action-server,action-client}/`
- `packages/testing/nros-tests/tests/zephyr.rs` — 4 new tests
- `just/zephyr.just` — 4 new build recipes

### 95.B — Zephyr dds-rust completeness

Same as 95.A but for DDS. Uses `qemu_cortex_a9` board overlay
established in Phase 92. Each test reuses
`zephyr::start_qemu_a9_mcast(...)`.

**Files**:
- `examples/zephyr/rust/dds/{service-server,service-client,action-server,action-client,async-service-client}/`
- `packages/testing/nros-tests/tests/zephyr.rs` — 5 new tests
- `just/zephyr.just` — 5 new build recipes

### 95.C / 95.D — Zephyr cpp-xrce / cpp-dds

Mirrors `examples/zephyr/cpp/zenoh/*` for xrce + dds. Each crate has a
`CMakeLists.txt` driven by `find_package(NanoRos)` and consumes the
generated cpp message types via `nano_ros_generate_interfaces(...
LANGUAGE CPP)`.

**Files**:
- `examples/zephyr/cpp/xrce/{talker,listener,service-server,service-client,action-server,action-client}/`
- `examples/zephyr/cpp/dds/{...same six...}/`
- `packages/testing/nros-tests/tests/zephyr.rs` — 12 new tests
- `just/zephyr.just` — 12 new build recipes (4 prefix groups: cpp-xrce,
  cpp-dds talker/listener pairs run through existing zenoh cpp recipe
  scaffolding)

### 95.E — Zephyr c-dds

Mirrors `examples/zephyr/c/xrce/*`. Six crates over DDS.

**Files**:
- `examples/zephyr/c/dds/{talker,listener,service-server,service-client,action-server,action-client}/`
- `packages/testing/nros-tests/tests/zephyr.rs` — 6 new tests
- `just/zephyr.just` — 6 new build recipes

### 95.F — Native dds-rust completeness

Port from `examples/native/rust/zenoh/{service-server,service-client,
action-server,action-client}`.

`async-service-client` and `lifecycle-node` deferred — DDS lifecycle
service interop with `rmw_cyclonedds_cpp` / `rmw_fastrtps_cpp` is
unverified; pull in only after a user asks.

**Files**:
- `examples/native/rust/dds/{service-server,service-client,action-server,action-client}/`
- `packages/testing/nros-tests/tests/native_dds.rs` (new file or
  extend existing `native.rs` with dds tests)

### 95.G / 95.H — Native c-dds / cpp-dds

Six crates each, mirroring `examples/native/{c,cpp}/zenoh/*`. CMake
build through `find_package(NanoRos)` with `RMW=dds` selection.

**Files**:
- `examples/native/c/dds/{...six...}/`
- `examples/native/cpp/dds/{...six...}/`
- nextest E2E tests (12 total)

### 95.I — Test integration

Wire all new tests into `just test-all`. Confirm per-platform nextest
groups still hold serial constraints (no port collisions). Update
`.config/nextest.toml` if a new group is needed (e.g., if dds-cpp
needs its own multicast slot).

### 95.J — Coverage verification

Re-run the matrix in this doc against the actual `examples/` tree.
All cells in scope flip to ✅. Out-of-scope cells (`async-*`,
`lifecycle-*`, `rtic-*` outside `(native, rust, zenoh)`) stay `—`.

## Acceptance Criteria

- [ ] All 52 new example crates build under `just <plat> build-fixtures`.
- [ ] All 52 new nextest E2E tests pass under `just test-all`.
- [ ] Each new crate has its own `.gitignore` per CLAUDE.md "Examples
      are Standalone Projects" rule.
- [ ] Each new crate's `Cargo.toml` / `CMakeLists.txt` reads the SDK
      paths from env vars / `-D` (no project-tree heuristics).
- [ ] Coverage matrix in this doc and in `book/src/getting-started/`
      examples table reflects the new cells.
- [ ] `just ci` passes.

## Notes

* **Mechanical, not architectural.** The boilerplate is the lesson —
  resist the urge to extract shared example helpers (CLAUDE.md
  forbids). Each crate stands alone.
* **Codegen artefacts gitignored.** `generated/` dirs in each new
  example are gitignored, recreated by `just generate-bindings`.
* **DDS service / action coverage on `nros-rmw-dds`.** The backend
  exposes `service.rs` (request/reply) and topics, which the
  `nros-node` action layer composes into the 5-channel action
  protocol. No DDS-side action work is required — but interop with
  `rmw_cyclonedds_cpp` / `rmw_fastrtps_cpp` action protocol is
  unverified. Tests are nano-ros ↔ nano-ros only.
* **Zephyr DDS surface.** Reuses Phase 92's `qemu_cortex_a9` build
  path. `native_sim` DDS (Phase 71.8 `[~]`) is not required for this
  phase — cortex_a9 is the canonical Zephyr DDS target.
* **Phasing.** Each group (A–H) is independent and can land
  separately. Recommend ordering A → F → C → E → B → G → D → H —
  finish RTOS-ready xrce first, then native DDS (already-validated
  POSIX path), then C++ ports, then DDS on Zephyr (riskiest).
