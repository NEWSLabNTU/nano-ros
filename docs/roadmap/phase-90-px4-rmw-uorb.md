# Phase 90 — PX4 RMW (nros-rmw-uorb) + nros-px4 board crate

**Goal**: Run nano-ros on PX4 Autopilot. Adds a uORB-based RMW and a
board-equivalent crate that hosts the nano-ros `Executor` inside a PX4
`ScheduledWorkItem`.

**Status**: **Complete** (v1 + 90.2b no_std registry + 90.4b
paired-topic services + 90.5b proper waker chain all landed). Phase
90 is feature-complete.

**Priority**: P1

**Depends on**: px4-rs project at phase 09+ (CMake integration + SITL
test fixture). Both vendored as git submodules under
`third-party/px4/` (Phase 98).

**Companion phase**: [Phase 98](phase-98-px4-autopilot-integration.md) —
PX4-Autopilot vendoring + SITL E2E test infrastructure.

## Overview

`px4-rs` is a standalone Rust async framework for PX4 modules (crates:
`px4-sys`, `px4-log`, `px4-workqueue`, `px4-uorb`, …). It contains no
nano-ros code. This phase adds the glue that lets nano-ros sit on top:

- `nros-rmw-uorb` — implements `nros-rmw` traits using `px4-uorb` as the
  underlying transport. Replaces zenoh / xrce-dds as the RMW choice.
- `nros-px4` — board-style crate. Mirrors `nros-mps2-an385` shape:
  exposes a `run(config, user_fn)` that spawns an `Executor` on a
  chosen PX4 WorkQueue. Style B entry point.

Style C (`async fn` with ROS names) is enabled by these two crates plus
`px4-workqueue`'s `#[task]` macro — no additional nano-ros code needed,
because `px4-workqueue` is style-agnostic.

## Architecture

```
user PX4 module (Style B)                 user PX4 module (Style C)
    │                                           │
    ├─ nros::Node  ─ nros-node                  ├─ nros::uorb::publication
    ├─ Executor::spin_once ─ nros-node          ├─ #[task]     ─ px4-workqueue
    ├─ nros_px4::run()      ─ nros-px4          └─ Subscription ─ nros-rmw-uorb
    └─ create_publisher_raw  ─ nros-node                │
       create_subscription_raw                          ▼
       try_loan / try_borrow (95.D arena)         px4-uorb  (px4-rs)
       SlotLending opt-in (95.F native)            px4-workqueue
                                                    px4-sys
```

ROS topic naming — `nros-rmw-uorb` maps ROS 2 topic names
(`/fmu/out/sensor_gyro`) to uORB topic IDs (`sensor_gyro`). This follows
the conventions used by `uxrce_dds_client` (`dds_topics.yaml`). The map
is generated from a TOML file (committed, not runtime).

## v1 Work items (all done)

### 90.1 — Workspace wiring ✅ `d09ef7da`

- [x] Add `packages/px4/nros-rmw-uorb/` + `packages/px4/nros-px4/` to
      the nano-ros workspace
- [x] Path deps point at `third-party/px4/px4-rs/crates/*` (vendored
      submodule per Phase 98; was symlink in initial commit, migrated
      to submodule in `841b4622`)
- [x] `.env.example` adds `PX4_AUTOPILOT_DIR` (PX4_RS_DIR removed when
      symlink → submodule)
- [x] `justfile` adds `px4` module group (`just px4 setup/doctor/test/
    build/build-sitl/test-sitl/ci`)

### 90.2 — `nros-rmw-uorb` skeleton ✅ `d09ef7da` + `54cb7489` + `d16abe42`

- [x] Cargo crate w/ `nros-rmw` as dep, `px4-uorb` + `px4-workqueue`
      under `feature = "std"` for host-mock testing
- [x] Implement `Session` trait (drive_io = no-op on uORB)
- [x] Implement `Publisher`, `Subscriber` traits via the typed-trampoline
      registry (`register::<T>(ros_name, instance)` enrols a typed
      `px4_uorb::Publication<T>` + `Subscription<T>`; `publish_raw` /
      `try_recv_raw` look up by ROS name)
- [x] Implement `Rmw` (returns `UorbSession`)
- [x] Direct typed `publication::<T>` / `subscription::<T>` API in
      `raw.rs` for users who want zero-overhead path bypassing nros-node
- [x] `nros::uorb` re-export in umbrella crate

### 90.3 — ROS topic → uORB topic mapping ✅ `d09ef7da` + `5326a9ea`

- [x] `packages/px4/nros-rmw-uorb/topics.toml` — initial mapping
      (10 PX4 standard topics + custom `sensor_ping` for examples)
- [x] `build.rs` turns TOML into `phf::Map<&'static str, TopicEntry>`
- [x] Unknown topic name → `TransportError::InvalidConfig`, not panic

### 90.4 — Service / Action stubs ✅ `d09ef7da`

- [x] First-cut `nros-rmw-uorb` supports **pub/sub only** —
      `UorbServiceServer` + `UorbServiceClient` return
      `TransportError::Backend("uORB: services not yet supported")`
- [x] Stubs are well-formed `ServiceServerTrait` / `ServiceClientTrait`
      impls so nros-node integration compiles unchanged
- [x] Native paired-topic + correlation-id protocol → see 90.4b below

### 90.5 — `nros-px4` board crate ✅ `54cb7489`

- [x] `Config` struct: `wq_name`, `node_name`, `namespace`, `domain_id`
- [x] `run<F>(config, |&Executor| -> Result<(), E>) -> !` matches
      existing board-crate signatures
- [x] Opens UorbSession-backed `Executor` via `Executor::open` w/
      `rmw-uorb` feature; user closure receives `&mut Executor` for
      node creation
- [x] WorkItem ScheduleNow waker integration → see 90.5b below

### 90.6 — First example (talker + listener) ✅ `3eaa36a5` + `5326a9ea`

- [x] `examples/px4/rust/uorb/talker/` — publishes `SensorPing` via
      direct `nros::uorb::publication` API
- [x] `examples/px4/rust/uorb/listener/` — subscribes via
      `nros::uorb::subscription`, logs each message via `px4-log`
- [x] Shared `examples/px4/rust/uorb/msg/SensorPing.msg` +
      `msg/CMakeLists.txt` w/ `config_msg_list_external` (PX4 must
      register custom topics so `o_id` resolves; without this the
      static OrbMetadata fallback w/ `o_id=u16::MAX` crashes the
      daemon — see Phase 98 commit `5326a9ea` for root-cause analysis)
- [x] CMake glue: `examples/px4/rust/uorb/src/CMakeLists.txt` lists
      `modules/nros_talker` + `modules/nros_listener`; each module's
      `src/modules/<name>/CMakeLists.txt` is a thin wrapper invoking
      `px4_rust_module()` against the standalone crate
- [x] `rust-toolchain.toml` pins nightly for `type_alias_impl_trait`
      required by `#[px4_workqueue::task]`

### 90.7 — SITL integration test ✅ `3eaa36a5` + `841b4622` + `5326a9ea`

- [x] `packages/testing/nros-tests/tests/px4_e2e.rs` — boots SITL,
      starts both modules, waits for listener's `recv:` log line
- [x] `px4-sitl` Cargo feature on nros-tests (opt-in regex +
      px4-sitl-tests path dev-dep)
- [x] Test PANICS (does not skip) when `PX4_AUTOPILOT_DIR` unset or
      vendored submodule unpopulated — per CLAUDE.md no-silent-skip rule
- [x] Reuses `Px4Sitl::boot_in()` from px4-sitl-tests fixture
      (subprocess drainer + line-tail w/ regex + SIGTERM-process-group
      cleanup) — only nros-side glue is the build-invocation wrapper
      pointing `EXTERNAL_MODULES_LOCATION` at our examples dir

### 90.8 — Docs ✅ `d16abe42`

- [x] `book/src/getting-started/px4.md` — install, build, run; covers
      both direct typed API (`nros::uorb::publication`) and Style B
      `Executor` integration
- [x] `docs/design/px4-rmw-uorb.md` — design rationale: why a separate
      RMW, Style B vs Style C, topic mapping, two API layers, memory
      model (memcpy not CDR), service/action gap, spin loop notes
- [x] CLAUDE.md phase table update — see 90.10

---

## v1 Acceptance criteria (all met)

- [x] `just px4 ci` passes (check + host-mock test) — workspace clean,
      10 nros-rmw-uorb tests green
- [x] Listener example receives messages from talker via SITL —
      verified end-to-end through real PX4 broker (`5326a9ea`)
- [x] `just ci` in the nano-ros workspace still green on non-PX4
      platforms — zpico + xrce + dds unaffected
- [x] Documentation: getting-started + design doc landed; CLAUDE.md
      phase table row added (90.10)

## v1 Verified flow

```
host:$ just px4 setup           # populates submodules (~500 MB shallow)
host:$ just px4 test-sitl       # build + boot SITL + assertions
        ...
        PASS [   2.85s] (1/1) nros-tests::px4_e2e
                              px4_sitl_talker_listener_round_trip
```

Cold cache: ~10 min (PX4 SITL build). Warm: ~3s.

---

## Post-v1 follow-ups

### 90.4b — Native services / actions over uORB ✅

Approach (a) from the original deferred plan: paired-topic +
correlation-id. Implemented as `UorbServiceServer` /
`UorbServiceClient` in `service.rs`.

**Wire format** on both `<svc>/_request` and `<svc>/_reply` topics:

```text
┌──────────────┬──────────────────┬───────────────────────┬──────────┐
│ seq: u64 LE  │ payload_len: u16 │ payload: payload_len  │ zero-pad │
│ (8 bytes)    │ LE (2 bytes)     │ bytes                 │ to o_size│
└──────────────┴──────────────────┴───────────────────────┴──────────┘
```

The trailing zero-pad exists because uORB's `publish_raw` requires
`data.len() == size_of::<T::Msg>()`. We always write a full
topic-sized buffer; the explicit `payload_len` lets the receiver
recover the meaningful prefix.

**Topic registration**: callers register the request and reply
topic markers manually via `register::<ReqT>`/`register::<ReplyT>`
before opening the service. Both topic structs MUST be exactly
`UORB_SERVICE_TOPIC_BYTES` (256 bytes) so the carrier matches the
wire format. Service name `/foo` is mapped internally to
`/foo/_request` and `/foo/_reply`.

**Limitations** (deliberate, documented in the module):

- Single in-flight request per client handle. Server happily
  accepts concurrent requests; client only matches the latest seq.
  The existing nros-node `Promise` shape doesn't support multi-flight
  anyway.
- Reply with non-matching seq is silently dropped — multiple client
  processes sharing one reply topic see only their own traffic.
- 246-byte payload cap (256-byte topic minus 10-byte header).
  Bumping `UORB_SERVICE_TOPIC_BYTES` is a recompile.

**Acceptance**:

- [x] `UorbServiceServer::try_recv_request` / `send_reply` impls
- [x] `UorbServiceClient::send_request_raw` / `try_recv_reply_raw`
      impls
- [x] `Session::create_service_server` / `create_service_client`
      wired (no longer return `TransportError::Backend("uORB:
      services not yet supported")`)
- [x] `tests/service_round_trip.rs` — 2 host-mock tests:
      `paired_topic_service_round_trip` (full request → reply via
      Session API) and `reply_for_other_seq_is_ignored` (verifies
      stale-seq rejection)
- [x] `just px4 ci` green; 15 host-mock tests total

### 90.5b — WorkItem ScheduleNow waker integration

Today `nros_px4::run` parks the executor in a 10 ms `spin_once` loop.
Functionally correct but burns CPU on quiescent topics. Replace with a
proper waker chain:

```
PX4 publish thread                      PX4 WQ thread
  └─ uORB broker                          └─ run_trampoline → poll_once
      └─ orb_callback(sub_cb)                  └─ poll(NrosTask)
          └─ wake_trampoline(&AtomicWaker)         └─ park future Ready
              └─ AtomicWaker::wake()                    └─ exec.spin_once(0)
                  └─ Waker::wake (WI vtable)            └─ park again
                      └─ ScheduleNow(WI handle) ────────┘
```

Three layers (all opt-in via `nros_px4::run_async`; the existing sync
`run()` path is unchanged for users who want it).

#### L1 — Per-topic waker hook in `nros-rmw-uorb` registry

`Handle<T>` already owns a `px4_uorb::Subscription<T>` whose
`AtomicWaker` is wired to `orb_register_callback` lazily on first
`try_recv()`. Two changes:

- Force `ensure_registered` at `register::<T>()` time so the callback
  exists from the start (avoids first-park race where the broker
  publishes before the lazy registration runs).
- Add `TopicHandle::register_wake(&core::task::Waker)` that delegates
  to `self.sub.waker.register(w)`. Trait method is `no_std`-clean.

Each registered topic still owns exactly one `AtomicWaker`. Multiple
parking awaiters would clobber each other; for `nros_px4::run_async`
there's exactly one executor task → exactly one waker → safe.

#### L2 — `park_until_event(max)` future in `nros-rmw-uorb`

```rust
pub async fn park_until_event(max: Duration) {
    Park { sleep: Sleep::new(max), polled: false }.await
}

impl Future for Park {
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        // Bounded sleep — caps timer-drift even if no uORB topic fires.
        if let Poll::Ready(()) = Pin::new(&mut self.sleep).poll(cx) {
            return Poll::Ready(());
        }
        // Register WI's waker on every active sub.
        for handle in registry().iter() {
            handle.register_wake(cx.waker());
        }
        Poll::Pending
    }
}
```

The bounded `Sleep` exists for two reasons: (a) nros timers must still
fire even with no uORB traffic, (b) `GuardCondition` wake is not yet
plumbed (see Caveats), so a periodic re-poll is the safety net.

#### L3 — `nros_px4::run_async` entry point

```rust
pub fn run_async<F, E>(config: Config<'static>, user_fn: F) -> !
where F: FnOnce(&mut Executor) -> Result<(), E>, E: Debug,
{
    let mut exec = Executor::open(...).expect("open");
    user_fn(&mut exec).expect("user_fn");

    static CELL: WorkItemCell<NrosTask> = WorkItemCell::new();
    CELL.spawn(nros_pump(exec, config.park_max),
               &wq_configurations::lp_default,
               c"nros_px4");

    loop { /* park main thread; PX4 module exit on shutdown */ }
}

async fn nros_pump(mut exec: Executor, park_max: Duration) {
    loop {
        let r = exec.spin_once(Duration::ZERO);
        if r.dispatched_count == 0 {
            nros_rmw_uorb::park_until_event(park_max).await;
        } else {
            px4_workqueue::yield_now().await;
        }
    }
}
```

`Config` gains `park_max: Duration` (default 50 ms = 20 Hz timer
accuracy ceiling on quiescent topics; uORB-driven topics stay
zero-latency event-driven regardless).

#### Knobs

| `park_max` | Trade-off                                                |
| ---------- | -------------------------------------------------------- |
| 50 ms      | nros timers ≤50 ms late; default                         |
| 1 s        | timer-light apps; near-zero idle CPU                     |
| `MAX`      | event-only; nros timers never fire (timer-free apps OK)  |

#### Caveats / not in scope

1. `GuardCondition::trigger` from non-WQ threads doesn't wake the park.
   Fix is ~30 LOC: registry holds one extra `AtomicWaker`; trigger
   calls it. Defer to follow-up unless user hits it.
2. `Executor::next_timer_deadline()` accessor would let `park` use
   actual next-timer instant instead of `park_max`. Cleanest, but
   touches `nros-node`. Bounded `park_max` is "good enough" for v1.
3. Service responses (90.4b) ride the same uORB callback path — no
   extra wake plumbing needed once 90.4b lands.

#### Acceptance

- [x] L1 — `TopicHandle::register_wake` + eager `ensure_registered`
- [x] L2 — `park_until_event(max)` future in `nros-rmw-uorb` (with
      two-state machine: arm + register on first poll, return Ready
      on any subsequent re-poll)
- [x] L3 — `nros_px4::run_async` + `Config::park_max` + `pump` /
      `pump_until` exposed for testing
- [x] Upstream px4-uorb gains `Subscription::register_waker` —
      committed `ee5dee3` on `jerry73204/px4-rs` main, submodule
      re-pinned in nano-ros
- [x] `just px4 ci` passes — clippy + 13 host-mock tests green (10
      nros-rmw-uorb + 3 nros-px4 pump E2E)
- [x] SITL E2E (`px4_sitl_talker_listener_round_trip`) still passes
      after submodule bump + L1 registry changes (2.81 s)
- [x] Pump E2E proves wake chain in host mock:
  - `park_until_event_wakes_on_uorb_publish` — uORB publish wakes
    park within 50–500 ms (vs 1 s `park_max`)
  - `pump_routes_publish_through_executor_within_park_window` —
    pump-driven Executor exits park promptly on publish
  - `pump_idles_on_quiescent_topics` — CPU < 200 ms over a 600 ms
    wall window (catches busy-loop regression)

#### Status of earlier limitations

- ~~`run_async` is std-only~~ — **resolved by Phase 90.2b** (see
  below). Trampoline registry now uses `critical_section::Mutex<RefCell<heapless::Vec>>`
  instead of `std::sync::Mutex<HashMap>` so the whole nros-rmw-uorb
  surface (registry + UorbSession + UorbPublisher + UorbSubscriber +
  park_until_event) is no_std + alloc compatible. Real PX4 NuttX
  modules can use `Executor` + `Node` directly. `nros_px4::run_async`
  itself remains std-only (Box::pin + std::thread::park + futures
  executor) but a no_std variant is straightforward follow-up work.
- ~~tcache_thread_shutdown crash with parallel tests~~ — **resolved
  upstream** in `px4-rs/3afaf2c2`: HRT mock migrated to a single
  shared worker thread; SubCbInner leak tracker added. `just px4
  test` runs default-parallel again; `--test-threads=1` workaround
  removed.

### 90.2b — no_std + alloc registry ✅

- [x] Replace `std::sync::Mutex<HashMap>` with
      `critical_section::Mutex<RefCell<heapless::Vec<MAX_TOPICS>>>`
- [x] Reshape `lookup` from guard-returning to closure-taking
      (`lookup_with(name, |handle| ...)`) — required by
      `critical_section::with`'s closure API
- [x] `register::<T>(...)` returns `Result<(), TransportError>`
      (full registry → `Backend("uORB registry full")` rather than
      panic). Capacity is `MAX_TOPICS = 32` const, recompile to bump.
- [x] Drop `cfg(feature = "std")` gates from registry, publisher,
      subscriber, park, service, session — all build no_std now.
- [x] `extern crate alloc` is unconditional (registry needs `Box`).
      Real PX4 NuttX has an allocator; bare-metal users must wire a
      `#[global_allocator]`.
- [x] Std builds activate `critical-section/std` so the global impl
      is available without the user wiring one. Real PX4 modules get
      a `critical_section` impl via px4-sys (interrupt-disable on
      NuttX).
- [x] Tests pass on both `cargo check -p nros-rmw-uorb` (no_std)
      and `cargo test --features std,test-helpers` (host).
- [x] SITL E2E (`px4_sitl_talker_listener_round_trip`) still passes
      (2.85 s).

### 90.10 — CLAUDE.md phase table ✅

- [x] Add Phase 90 row to CLAUDE.md table marking it v1 Complete with
      pointer to this doc + Phase 98 companion.

---

## Notes

- Do not implement uORB as a variant inside `nros-rmw-zenoh` or
  `nros-rmw-xrce`. It's a separate RMW with its own semantics
  (level-triggered, in-process, no CDR). Same dir-level split pattern
  as `packages/zpico/` vs. `packages/xrce/`.
- "Zero-copy" is misleading at the orb_publish layer — uORB always
  memcpys into the kernel ring. The Phase 95 loan/borrow API
  eliminates user-side copies (`PublishLoan` arena slot lives in the
  publisher, user writes directly into it; commit calls
  `publish_raw` which goes through the same trampoline + orb_publish).
- The executor affinity caveat from `px4-rs/docs/async-model.md`
  applies: one `Executor` pins to one WQ. Nodes with hard-rate + slow
  work should split across multiple `Executor` instances, one per WQ.
- Custom topics MUST be registered via `msg/CMakeLists.txt`'s
  `config_msg_list_external` so PX4 assigns a valid `o_id`. The
  Rust-side static OrbMetadata fallback w/ `o_id=u16::MAX` crashes
  PX4's broker on subscriber registration. See Phase 98 commit
  `5326a9ea` for the full root-cause walkthrough.

## Risks (resolved)

- **uORB topic metadata layout drift** — px4-sys submodule pin
  protects against ABI breakage. Will re-verify on each px4-rs
  submodule bump.
- **Service/action semantics** — deferred to 90.4b. v1 explicit error
  is the right shape until user demand crystallises.
- **PX4 build integration** — verified working end-to-end via Phase 98
  SITL test (`5326a9ea`).

## Prerequisites checklist (verify before starting)

- [x] `third-party/px4/px4-rs` submodule populated (Phase 98.1)
- [x] `third-party/px4/PX4-Autopilot` submodule populated recursively
      (Phase 98.2)
- [x] `px4-uorb::Subscription<M>::recv()` exercised via the
      `loan_borrow.rs` + `typed_pubsub.rs` host-mock tests
- [x] `~/repos/PX4-Autopilot/` no longer needed — vendored submodule
      replaces it. `PX4_AUTOPILOT_DIR` env override still respected
      for users w/ pre-existing checkouts.
