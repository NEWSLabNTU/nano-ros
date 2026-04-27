# Phase 90 — PX4 RMW (nros-rmw-uorb) + nros-px4 board crate

**Goal**: Run nano-ros on PX4 Autopilot. Adds a uORB-based RMW and a
board-equivalent crate that hosts the nano-ros `Executor` inside a PX4
`ScheduledWorkItem`.

**Status**: Not Started
**Priority**: P1
**Depends on**: px4-rs project reaching its phase 07 (CMake integration +
first Pixhawk module). See `~/repos/px4-rs/docs/roadmap/`.

## Overview

`px4-rs` is a standalone Rust async framework for PX4 modules (crates:
`px4-sys`, `px4-log`, `px4-workqueue`, `px4-uorb`, …). It contains no
nano-ros code. This phase adds the glue that lets nano-ros sit on top:

- `nros-rmw-uorb` — implements `nros-rmw` traits using `px4-uorb` as the
  underlying transport. Replaces zenoh / xrce-dds as the RMW choice.
- `nros-px4` — board-style crate. Mirrors `nros-mps2-an385` shape:
  exposes a `run(config, user_fn)` that wires an `Executor` into a
  `ScheduledWorkItem` on a chosen PX4 WorkQueue. Style B entry point.

Style C (`async fn` with ROS names) is enabled by these two crates plus
`px4-workqueue`'s `#[task]` macro — no additional nano-ros code needed,
because `px4-workqueue` is style-agnostic.

## Architecture

```
user PX4 module (Style B)                 user PX4 module (Style C)
    │                                           │
    ├─ nros::Node  ─ nros-node                  ├─ nros::Node ─ nros-node
    ├─ Executor::spin_once ─ nros-node          ├─ #[task]     ─ px4-workqueue
    ├─ nros_px4::run()      ─ nros-px4          └─ Subscription ─ nros-rmw-uorb
    └─ Subscription<M>      ─ nros-rmw-uorb              │
                                                          ▼
                                                       px4-uorb  (px4-rs)
                                                       px4-workqueue
                                                       px4-sys
```

ROS topic naming — `nros-rmw-uorb` maps ROS 2 topic names
(`/fmu/out/sensor_gyro`) to uORB topic IDs (`sensor_gyro`). This follows
the conventions used by `uxrce_dds_client` (`dds_topics.yaml`). The map
is generated from a simple TOML file (committed, not runtime).

## Work items

### 90.1 — Workspace wiring

- [ ] Add `packages/px4/nros-rmw-uorb/` + `packages/px4/nros-px4/` to
      the nano-ros workspace
- [ ] Root `Cargo.toml` patches `px4-uorb` / `px4-workqueue` / `px4-sys`
      at `path = "../../../px4-rs/crates/*"` for local dev
- [ ] `.env.example` adds `PX4_RS_DIR` pointing at `~/repos/px4-rs`
- [ ] `justfile` adds a `px4` module group (`just px4 setup/doctor/test`)
      following the pattern of `freertos`, `nuttx`, `threadx_linux`

### 90.2 — `nros-rmw-uorb` skeleton

- [ ] Cargo crate with `nros-rmw` as dep, `px4-uorb` + `px4-workqueue`
      under `cfg(target_os = "nuttx")` or a feature flag
- [ ] Implement `trait Session` — `drive_io` is a no-op on uORB
- [ ] Implement `trait Publisher<M>` over `px4_uorb::Publication<M>`
- [ ] Implement `trait Subscriber<M>` over `px4_uorb::Subscription<M>`
      with arena-compatible callback glue (uORB cb → nano-ros
      readiness bit + waker on the hosting WorkItem)
- [ ] `Transport` / `Rmw` impls minimal enough to satisfy `nros-node`

### 90.3 — ROS topic → uORB topic mapping

- [ ] `packages/px4/nros-rmw-uorb/topics.toml` — initial mapping copied
      from `uxrce_dds_client`'s `dds_topics.yaml` (compact subset for
      phase entry)
- [ ] build.rs turns the TOML into a `phf::Map<&'static str,
    &'static orb_metadata>`
- [ ] Unknown topic name → `TransportError::InvalidTopic`, not panic

### 90.4 — Service / Action semantics

- [ ] Document that first-cut `nros-rmw-uorb` supports **pub/sub and
      timers only** — services/actions return `Err(NotSupported)`
- [ ] Stub out the service/action trait methods with clear TODOs so
      later phases can fill them in (likely over a paired request/reply
      uORB topic convention or an orthogonal RPC channel)

### 90.5 — `nros-px4` board crate

- [ ] `Config` struct with fields: `wq_name`, `node_name`, `namespace`,
      `domain_id` (currently unused — kept for API compatibility)
- [ ] `run<F>(config, |cfg| -> Result<(), NodeError>) -> !` — signature
      matches existing board crates (`nros-mps2-an385::run`,
      `nros-nuttx-qemu-arm::run`)
- [ ] Internally: attach a `NrosWorkItem` (subclass of
      `px4_workqueue::WorkItemCell`) to the chosen WQ, install a wake
      callback into `nros-rmw-uorb` so uORB publishes trigger
      `ScheduleNow()`, spin on the executor inside `Run()`

### 90.6 — First example

- [ ] `examples/px4/rust/uorb/listener/` — subscribes to a PX4 topic
      using ROS 2 naming, logs via `px4-log`
- [ ] `examples/px4/rust/uorb/talker/` — publishes at 10 Hz
- [ ] `examples/px4/config.toml` picking WQ + topic name mappings
- [ ] CMake glue that copies `px4-rust`'s `px4_rust_module()` function
      (or imports it) — first shared example between the two projects

### 90.7 — Integration test

- [ ] `packages/testing/nros-tests/tests/px4_e2e.rs` — spins up PX4 SITL
      (jmavsim or gazebo headless), loads the listener + talker modules,
      verifies N messages delivered. Gated behind
      `cfg(feature = "px4-sitl")` + precondition check for
      `PX4_AUTOPILOT_DIR` + SITL binary availability.
- [ ] Platform group in nextest config: `px4` with `max-threads = 1`
- [ ] Test must **fail** (not skip) if SITL is unavailable but the
      feature is enabled — per the project-wide "no silent skip" rule

### 90.8 — Docs

- [ ] `book/src/getting-started/px4.md` — install, build, run
- [ ] `docs/design/px4-rmw-uorb.md` — link to
      `px4-rs/docs/architecture.md` for the async model; focus this doc
      on the nano-ros-specific choices (topic-name mapping, Style B
      vs. C trade-off, service/action gaps)
- [ ] Update `CLAUDE.md` phase table and "Platform Backends" list

## Acceptance criteria

- [ ] `just px4 ci` passes (check + test) on a machine with PX4 SITL
      installed
- [ ] Style B example (listener) receives messages from a PX4 SITL topic
- [ ] Style C example (same logic via `#[task]` + `async`) works
      end-to-end using only px4-rs + nros-rmw-uorb — no new nano-ros
      runtime code
- [ ] `just ci` in the nano-ros workspace is still green on non-PX4
      platforms (zpico + xrce unaffected)
- [ ] Documentation updated: CLAUDE.md platform backends list includes
      uORB; book has a PX4 getting-started page

## Notes

- Do not implement uORB as a variant inside `nros-rmw-zenoh` or
  `nros-rmw-xrce`. It's a separate RMW with its own semantics (level-
  triggered, in-process, no CDR). Same dir-level split pattern as
  `packages/zpico/` vs. `packages/xrce/`.
- "Zero-copy" is misleading — `orb_copy` memcpys into the subscriber.
  Prose should say "serialization-free" to match reality.
- The executor affinity caveat from `px4-rs/docs/async-model.md`
  applies: one `Executor` pins to one WQ. Nodes with hard-rate + slow
  work split across multiple `Executor` instances, one per WQ.

## Risks

- **uORB topic metadata layout drift** — if PX4 changes
  `orb_metadata`'s ABI, `px4-sys` breaks and we rebuild bindings. Low
  risk; the struct has been stable for years.
- **Service/action semantics** — no obvious uORB-native mapping. May
  force us to layer a small RPC protocol on top of paired request/reply
  topics, or recommend users do services over XRCE while keeping pub/sub
  native. Decide during 90.4.
- **PX4 build integration** — Pictorus proves it works; our `px4-rs`
  phase 07 de-risks it before this phase starts.

## Prerequisites checklist (verify before starting)

- [ ] `~/repos/px4-rs/` at or past phase 07 (CMake integration)
- [ ] `px4-uorb::Subscription<M>::recv()` demonstrated on a real/QEMU
      Pixhawk in a `gyro_watch`-style example
- [ ] `~/repos/PX4-Autopilot/` checkout available for codegen + SITL
