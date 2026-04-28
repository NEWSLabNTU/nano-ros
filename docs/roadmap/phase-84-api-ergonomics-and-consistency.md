# Phase 84: API Ergonomics and Consistency Pass

**Goal**: Resolve the design flaws and UX issues surfaced by the April 2026
five-surface API audit (C, C++, Rust, RMW, Platform). The phase is a
collection of independently-landable groups, each with a bounded blast
radius. It is not a single monolithic refactor.

**Status**: In Progress (started 2026-04-19). As of 2026-04-24:
Groups A, B, C, D, E, G complete. Group F complete except **F6**
(directory / board-crate rename — scheduled last). B4's REP-2002
service exposure half was closed by Phase 86 (`nros-lifecycle-msgs`
codegen + executor-integrated lifecycle services + C FFI). E2b
landed as two PRs (E2b.1 state struct + SharedCell; E2b.2
args-threaded callbacks); E2b.3 (multi-session `Box`-owned state)
abandoned to preserve alloc-free XRCE support on Zephyr. F4 landed
as six sub-PRs (F4.1 trait freeze; F4.2 Clock+Alloc; F4.3
Sleep+Random+Time; F4.4 network traits; F4.5 Threading; F4.6
Libc+NetworkPoll documentary). G8 landed 2026-04-24 as the
per-entity header-split that moves each `Node::create_X<T>`
template definition next to its entity class. **Still open**: F6.
**Priority**: Medium — no single finding blocks users, but the debt is
compounding and several items (thin-wrapper violations, documentation drift,
silent footguns) are already surfacing in issues / example debugging sessions.
**Depends on**: Phase 77 (async action client — closes several blocking-flag
footguns), Phase 80 (unified network interface — will absorb Group F's
`set_network_state` globals).

## Overview

An independent audit ran five parallel reviews — one per API surface — with
the explicit goal of flagging design flaws and UX issues. Findings cluster
into six groups plus a small bucket of nits. Each group lands as its own PR
(or small PR series). Open questions are explicitly called out — several
require user judgement before implementation.

### Audit sources

- C API: `packages/core/nros-c/` + headers + examples + `book/src/reference/c-api.md`.
- C++ API: `packages/core/nros-cpp/` + headers + examples + `book/src/reference/cpp-api.md`.
- Rust API: `packages/core/nros/`, `nros-node`, `nros-core`, `nros-params`, `nros-serdes` + `book/src/reference/rust-api.md`.
- RMW layer: `packages/core/nros-rmw/`, `packages/zpico/nros-rmw-zenoh/`, `packages/xrce/nros-rmw-xrce/`, `book/src/porting/custom-rmw.md`.
- Platform layer: `packages/core/nros-platform/`, `packages/{core,boards}/nros-platform-*`, `zpico-platform-*`, `xrce-platform-*`, `book/src/porting/custom-platform.md`.

## Work Items (by group)

Group titles use letters (A–G) so phase numbering tracks sub-items within a
group. All open design questions from the April 2026 audit are now resolved;
§Open Questions below preserves the trade-off analysis for future reference.

### Group A — Documentation drift (book references)

- [x] 84.A1 — Rewrite `book/src/reference/rust-api.md` from real source
- [x] 84.A2 — Rewrite `book/src/reference/c-api.md` top sections from real source
- [x] 84.A3 — Add `cargo test --doc -p nros` gate to CI to keep Rust doctests honest
- [x] 84.A4 — Audit `book/src/porting/custom-rmw.md` for missing sections (liveliness, attachments, action's 5-channel layout)
- [x] 84.A5 — Audit `book/src/porting/custom-platform.md` for post-Phase 79 freshness (cffi vtable, xrce-platform-shim, feature flags, `DEP_ZPICO_SOCKET_SIZE`)

### Group B — C API thin-wrapper compliance

Biggest design debt. Every item is multi-day because `nros-node` must grow
the real implementation first.

- [x] 84.B1 — Fix `nros_service_init` / `nros_client_init` to take `nros_service_type_t*` instead of `nros_message_type_t*`
- [x] 84.B2 — Delete `packages/core/nros-c/src/cdr.rs` (1187 lines); delegate to `nros_serdes::{CdrReader, CdrWriter}`. Landed as thin FFI bridges over positioned `CdrReader::new_at` / `CdrWriter::new_at` constructors added to `nros-serdes`. Line count 1187 → 627 (hand-rolled align/endian logic removed; FFI tests kept; kani harnesses dropped since the logic now lives in `nros-serdes`, already covered there).
- [x] 84.B3 — **Done in two passes**. Pass 1: `impl_param_scalar!` macro mirroring the existing `impl_param_array!`, collapsing `nros_param_{declare,get,set}_{bool,integer,double}` to one ~60-line macro instance each (parameter.rs 1175 → 1055, -10%). Pass 2: real service wiring via a new `nros_executor_register_parameter_services(exec)` FFI + `nros_executor_{declare,get,set}_param_{bool,integer,double,string}` + `nros_executor_has_param`, all gated on a new `param-services` Cargo feature on `nros-c` (forwards to `nros-node::param-services` + needs `alloc`). The new API operates on the `nros-params::ParameterServer` owned by the `Executor`, so a C-declared parameter is now visible to `ros2 param list /<node>` once services are registered. The legacy `nros_param_server_t` + its array storage is left in place for backwards compatibility; new code should use the executor-backed path. `SetParameterResult` now re-exported from `nros-node` for FFI consumers.
- [x] 84.B4 — **Complete** (two landings). First landing: state-machine logic moved to `nros_node::lifecycle::LifecyclePollingNodeCtx` (new C-FFI-compatible variant of the existing `LifecyclePollingNode`). C wrapper `nros_lifecycle_state_machine_t` is now an opaque u64-storage struct that holds the Rust type inline; all register / trigger / finalise paths delegate. ABI-breaking: C callers can no longer reach into `current_state` / `on_configure` / etc. — they must use `nros_lifecycle_get_state()` + `nros_lifecycle_register_on_*()`. Line count 703 → 430 (-39%). State-machine tests moved to `nros_node::lifecycle::tests`. Second landing (REP-2002 service exposure): **closed by Phase 86** — see [`docs/roadmap/archived/phase-86-nros-lifecycle-msgs.md`](archived/phase-86-nros-lifecycle-msgs.md). Phase 86 added the `nros-lifecycle-msgs` codegen crate (86.1), the `nros_node::lifecycle_services` module wiring the five REP-2002 services into `Executor::spin_once` (86.2–86.3), and the C FFI (`nros_executor_register_lifecycle_services` + handle accessors in `nros-c/src/lifecycle.rs`, 86.4), with serde round-trip tests (86.7), integration tests (86.8), and live ROS 2 interop verified against a pinned `rmw_zenoh_cpp` (86.9–86.10).
- [x] 84.B5 — Hide or remove `nros_timer_call` / `nros_timer_is_ready` + mutable `period_ns` / `last_call_time_ns` fields (executor-internal). Dead FFI functions removed entirely (no internal callers). Struct fields tagged `@internal` in the C header pointing users to `nros_timer_get_period` / `nros_timer_set_period` accessors instead of a field-name break.
- [x] 84.B6 — Add `NROS_RET_REENTRANT` (= -15) to public `nros/types.h` (also added `NROS_RET_TRY_AGAIN = -14` which was missing)

### Group C — C++ move-safety and error propagation

- [x] 84.C1 — Added `nros_cpp_<type>_relocate(old, new)` FFI for all 7 opaque-storage C++ types (`Publisher`, `Subscription`, `Service`, `Client`, `GuardCondition`, `ActionServer`, `ActionClient`). Each function does `ptr::read(old) + ptr::write(new)` inside `nros-cpp` (no `nros-node` handle API churn needed — the audit found that only `ActionServer` actually registers its storage address externally, via `this` as the arena callback context). C++ move ctors / `operator=` all go through `_relocate` instead of raw `memcpy`, which gives a uniform bridge point if any type later gains an arena-registered self-pointer. `ActionServer`'s existing `install_callbacks()` re-register step is kept — `_relocate` transfers the Rust-side state, then C++ updates the callback `ctx` with the new `this`. `Timer` doesn't need `_relocate` (its state is `handle_id` + stable executor pointer + optional `unique_ptr<std::function>` — all move-safe by definition); `Executor` is a top-level handle that isn't moved post-init. All 348 workspace tests pass and all 6 native C++ examples build clean.
- [x] 84.C2 — Fix `std::function` trampoline leak in `std_compat.hpp:46-81`: attach closure ownership to the Timer/GuardCondition C++ instance (e.g. `std::unique_ptr<std::function<…>>` field), pass raw pointer to Rust, free in destructor
- [x] 84.C3 — `Future<T>::wait` / `Stream<T>::wait_next` must propagate non-transient `spin_once` errors (currently discards every return)
- [x] 84.C4 — `ActionClient::set_callbacks` returns `Result`, not `void`
- [x] 84.C5 — Expose `poll_ms` on `Future::wait` / `Stream::wait_next` (currently hard-coded to 10 ms)
- [x] 84.C6 — Add `static_assert` on `set_goal_callback` / `set_cancel_callback` / `for_each_active_goal` requiring stateless callables, with a targeted error message
- [x] 84.C7 — Auto-generate every `NROS_CPP_*_STORAGE_SIZE` macro in `nros_cpp_config_generated.h` (publisher/subscription/service/guard were hand-rolled; executor/action already auto-generated). build.rs now emits all 7 size macros; config.hpp is a 1-line wrapper that includes the generated header. Rust-side compile-time asserts ensure `size_of::<T>()` fits each generated byte count — any future type growth fails the build with a targeted message instead of silently overflowing caller storage.
- [x] 84.C8 — `Executor::handle()` should be non-const (it returns a mutable pointer via `const_cast`)
- [x] 84.C9 — Add `[[deprecated("use send_request().wait()")]]` on blocking `Client::call` / `call_raw`
- [x] 84.C10 — Rationalize `try_recv` return types: **always distinguish "no message" from "deserialize error"**. Change every `try_recv*` / `try_recv_request` / `try_recv_feedback` to return `Result` (with a `TryAgain` variant for "no message"). Drop the `bool` overloads entirely rather than keeping parallel APIs — the behavioural difference between `false = nothing` and `false = deserialize-failed` is too subtle to leave exposed. *(`Result` has `explicit operator bool` so existing `if (sub.try_recv(msg))` call sites still compile; they now correctly break on deserialize errors instead of looping.)*

### Group D — Rust API: error types, action server poll path, single-slot clients

- [x] 84.D1 — Unify error types **(first pass landed; NanoRosError consolidation deferred)**. What's done: `NodeError` is confirmed as the single user-facing error in every `nros-node` return signature; added `NodeError::Deserialization` variant + `From<SerError>` and `From<DeserError>` impls so `?` now bubbles serialization errors from user `impl RosMessage` code into user function bodies that return `Result<_, NodeError>`. What's deferred: folding `NanoRosError` (with topic/service-name context) into `NodeError`, hiding `TransportError` from `nros::*` (it's still needed for matching on the inner error). The deferred work has its own phase entry — treat this checkbox as "ergonomic bubble-up fix complete", not "full unification".
- [x] 84.D2 — Added `ActionServer::poll(on_goal, on_cancel)` that fuses `try_accept_goal` + `try_handle_cancel` + `try_handle_get_result` into one call. Book's manual-poll server section now recommends `poll()` as the default path (the three granular methods are kept for users who need finer control). Auto-drain during `spin_once` is NOT done — manual-poll servers are intentionally not arena-registered, so `spin_once` wouldn't know about them; fused `poll()` is the pragmatic fix.
- [x] 84.D3 — **Landed**. Added `in_flight: bool` to `EmbeddedServiceClient`, plus three per-sub-client flags on `ActionClientCore` (`in_flight_send_goal`, `in_flight_cancel`, `in_flight_get_result`). `call()` / `send_goal()` / `cancel_goal()` / `get_result()` return the new `NodeError::RequestInFlight` if the previous reply hasn't been consumed. `Promise` now carries `in_flight_flag: &mut bool`; `Promise::try_recv()` clears it on a successful reply. Dropping a `Promise` without awaiting leaves the flag set — the caller must either poll the existing promise or explicitly call `reset_in_flight()` / `reset_send_goal_in_flight()` / `reset_cancel_in_flight()` / `reset_get_result_in_flight()` to acknowledge the abandoned call. This closes the hazard where a stale reply from an abandoned call landed on the next one.
- [x] 84.D4 — `QosReliabilityPolicy::default()` returns `BestEffort` while `QosSettings::default()` is Reliable. Make the enum default match ROS 2 (`Reliable`).
- [x] 84.D5 — Expand `prelude` with `Promise`, `TransportError`, manual-poll `Subscription`, `Publisher`, `HandleId`, `SpinOnceResult`
- [x] 84.D6 — Replace `ExecutorConfig::from_env()`'s `Box::leak` with owned strings (gate behind `std`)
- [x] 84.D7 — **Mass signature migration landed.** All public wait APIs on `Executor` / `Promise` / `Subscription` / `FeedbackStream` / `GoalFeedbackStream` now take `core::time::Duration` instead of `i32 ms` / `u64 ms`. The `spin_once(-1)` timer-freeze footgun is gone by construction: `Duration` has no negative sentinel, and the internal conversion clamps to `[0, i32::MAX]` ms before calling the transport's `drive_io`. Migrated sites: every example, every test, the C FFI internal conversion (`nros_cpp_spin_once` / `nros_executor_spin_once` keep their integer C ABI and convert on entry). The old parallel `_for` / `_with_period` names introduced in the earlier partial pass were removed — there is now one canonical name per API. **Deferred (genuinely separate PR)**: swapping the timer-delta source from "reuse the poll-timeout value" to "measure with a wall clock (nros-platform::clock_ms)". That change needs nros-node to gain a cross-crate dependency on nros-platform, which is a structural decision worth its own PR; it's orthogonal to the `Duration` signature migration.
- [x] 84.D8 — `executor.parameter::<T>("name")` should return `Result<ParameterBuilder, NodeError>` instead of `expect`ing
- [x] 84.D9 — `Subscription<M>` gains `recv().await` (async, no futures dep), `wait_next(&mut executor, timeout_ms) -> Result<Option<M>, NodeError>` (sync, spins executor), and `futures_core::Stream` impl gated on the `stream` feature. Methods land directly on `Subscription` (Option A from the design discussion) — no separate `MessageStream` type, since subscriptions ARE the message receiver. Also closes the AtomicWaker race in `Promise::poll`, `FeedbackStream::recv` / `poll_next`, and `GoalFeedbackStream::recv` / `poll_next` — the previous `try_recv → if None then register_waker` pattern had a window where a callback firing between the check and the register would deliver the wake to no waker, hanging the task forever. Switched all sites to the canonical `register_waker → then check` order.
- [x] 84.D10 — Always export `PublisherOptions` / `SubscriberOptions` unconditionally. An RMW feature is always selected (backend is mandatory at compile time), so the `cfg(not(feature = "rmw-zenoh"))` gate is strictly wrong for XRCE/DDS users today. Remove all three cfg gates.

### Group E — RMW: backend asymmetry, static state, open signature

- [x] 84.E1 — XRCE backend: explicit `try_recv_raw_with_info` override that makes the "no per-sample info yet" gap visible in code (behaviour matches the trait default of `(len, None)`, but the override carries a doc comment explaining the missing plumbing and pointing at the follow-up). `safety-e2e` feature wiring added to `nros-rmw-xrce` + cascaded through `nros-node` and `nros` so users opting into `safety-e2e` with XRCE now get a clean feature graph (validation result reports `crc_valid: None` until sample info is plumbed through the callback). `nros-rmw` also re-exports `MessageInfo` so third-party backends don't need a direct `nros-core` dep to implement the trait.
- [x] 84.E2a — **Landed**. `Rmw::open` is now `fn open(self, config: &RmwConfig)` — consumes `self`. All four in-repo backends (`ZenohRmw`, `XrceRmw`, `DdsRmw`, `CffiRmw`) implement `Default`, so the canonical call site is `BackendRmw::default().open(&config)`. `XrceRmw::with_agent([u8; 4], u16)` added as a forward-compat factory constructor (the existing locator-string init hook still carries the actual agent address; `with_agent` is wired for backends that want to move to factory-level configuration). Callers updated in `Executor::open` (all 4 cfg-gated blocks), `nros::open_session` (same 4), and `nros-tests/tests/rmw.rs` (6 call sites). The `RmwLegacy` blanket-impl shim and `backend: ConcreteRmw` field on `ExecutorConfig` described in the original spec were **not** added — no in-repo third-party backend needs the deprecation shim, and the cfg-gated selection in `Executor::open` already handles backend multiplexing without a typed field.
- [x] 84.E2b — **Complete** (landed 2026-04-24 as two PRs). The 11
      module globals (`TRANSPORT`, `SESSION`, `OUTPUT_RELIABLE`,
      `INPUT_RELIABLE`, `PARTICIPANT_ID`, `NEXT_ENTITY_ID`,
      `INITIALIZED`, `OUTPUT_RELIABLE_BUF`, `INPUT_RELIABLE_BUF`,
      `SUBSCRIBER_SLOTS`, `SERVICE_SERVER_SLOTS`,
      `SERVICE_CLIENT_SLOTS`) are now typed fields of
      `XrceSessionState`, held at module scope behind a
      `SharedCell<T>` (`UnsafeCell<T>` + hand-rolled `Sync`).
      Callbacks reach the state through their `args: *mut c_void`
      parameter, installed by `XrceRmw::open`. `static mut` is gone
      from the crate (the `#![allow(static_mut_refs)]` attribute
      was removed). E2a delivered the compatible trait shape
      (`open(self, …)` consumes the factory) so multi-session
      remains architecturally reachable but is **not** part of this
      phase — the stretch PR (E2b.3) was abandoned to preserve
      alloc-free support, see below. Sub-PRs:

      - [x] **E2b.1 — state struct + no-op migration.** Defined
        `XrceSessionState` holding all 11 fields, wrapped it in a
        `SharedCell<T>` (`UnsafeCell<T>` + hand-rolled `Sync` impl
        justified by the existing `ffi_guard` discipline), collapsed
        the 11 `static mut` declarations into one
        `static STATE: SharedCell<XrceSessionState>`, and rewrote
        107 call sites to go through an `unsafe fn state()`
        accessor. Behaviour byte-identical (single-session; module
        storage); only `static mut` is gone. The
        `#![allow(static_mut_refs)]` attribute was removed. 14 XRCE
        E2E + 5 C XRCE API tests pass post-change.
      - [x] **E2b.2 — args-threaded callbacks.** Added
        `unsafe fn state_from_args(args)` helper; the three
        callbacks (`topic_callback`, `request_callback`,
        `reply_callback`) now reach the state through their `args`
        parameter and no longer touch the module accessor. Pointer
        is installed by `XrceRmw::open` via
        `state() as *mut XrceSessionState as *mut c_void`. Still
        single-session (state still lives at module scope behind
        `SharedCell`); the callback API is now decoupled from the
        global so E2b.3 only has to move the state's storage site.
      - [~] **E2b.3 — `Box`-owned state under `alloc` feature —
        abandoned 2026-04-24.** Rationale: moving
        `XrceSessionState` into `Box<XrceSessionState>` would force
        alloc on the Zephyr XRCE path (see
        `examples/zephyr/rust/xrce/talker/Cargo.toml` — uses
        `default-features = false, features = ["rmw-xrce",
        "platform-zephyr"]`, no global allocator registered). A
        feature-gated Box path would work but maintains two
        storage shapes with diverging `Drop` semantics; the only
        in-repo customer needing multi-session is a hypothetical
        multi-agent test harness that can trivially run as separate
        processes today. When a concrete user materializes, reopen
        under an `alloc`-gated path — PR 2's decoupled callbacks
        already isolate the callback surface from the storage
        site, so the migration is mechanical:

        - Add `XrceSession { #[cfg(feature = "alloc")] state:
          alloc::boxed::Box<XrceSessionState> }`
        - `XrceRmw::open` branches on cfg: alloc path
          `Box::new(XrceSessionState::zeroed())`, no-alloc path
          keeps the module `SharedCell`
        - Callbacks already take `args` → nothing to change there
        - Add multi-session regression test behind `alloc`

      Scope landed: E2b.1 (~380 lines touched, zero net growth)
      and E2b.2 (~100 lines touched). Abandoned E2b.3 would have
      added ~260 new lines + the feature split.
- [x] 84.E2c — **Landed**. `book/src/porting/custom-rmw.md` now documents the Phase 84.E2 factory shape: every backend is a value type, implements `Default`, and consumes `self` in `open` so pre-open state moves into the `Session`. Includes the "each backend reads its own env vars via `<Backend>::from_env()`" convention and an explicit "no `static mut` session-global state" guideline (multi-session test is the acceptance check).
- [x] 84.E3 — Rename `ZENOH_LOCATOR` / `ZENOH_MODE` env vars to `NROS_LOCATOR` / `NROS_SESSION_MODE`; accept legacy names with a deprecation warning. Update `book/src/reference/environment-variables.md`.
- [x] 84.E4 — Add `properties: &'a [(&'a str, &'a str)]` to `RmwConfig` so backend-specific config (TLS certs, multicast scouting, XRCE agent port) has a uniform channel
- [x] 84.E5 — Drop the 500k-iter busy-loop default body from `call_raw` / `call<S>` on the `Transport` trait — either force impls or return `TransportError::Timeout` immediately
- [x] 84.E6 — Flip `nros-rmw` default features to no-std (`default = []`); anyone relying on `std` must opt in. Use crate-internal `sync::Mutex` in the zenoh shim instead of `std::sync::Mutex`. *(Default-flip done; zenoh shim's `EXECUTOR_WAKE` kept on `std::sync::Mutex + Condvar` because the crate's `sync::Mutex` is a closure-based `with()` API with no condvar support — replacing it would require adding condvar primitives to `nros-rmw::sync`, which is out of scope.)*
- [x] 84.E7 — Remove the 1 KB stack buffer default for `process_raw_in_place` on the `Subscriber` trait; force backends to implement (or return `MessageTooLarge`)
- [x] 84.E8 — Remove the no-op default for `Session::drive_io`; both shipped backends are pull-based, so the default is a trap for a third implementer
- [x] 84.E9 — Remove `ZenohZeroCopySubscriber` from the public `nros-rmw-zenoh` surface. The doc already says "Deprecated" and board crates pull in `unstable-zenoh-api` only because of this re-export. Drop the type, drop the re-export, audit board Cargo.tomls for the now-unused feature. *(Feature itself kept — still used by `add_subscription_buffered_raw` via zpico-sys. No board Cargo.tomls needed cleanup.)*
- [x] 84.E10 — Add `Udp` variant to `locator_protocol` / `validate_locator`, or move locator parsing into backend crates entirely
- [x] 84.E11 — Remove `Copy` from `TransportError` (keep `Clone + Debug + PartialEq + Eq`). Add `Backend(&'static str)` variant unconditionally + `BackendDynamic(alloc::string::String)` gated on the `alloc` feature. `NodeError` loses `Copy` too (it wraps `TransportError::Transport`). No existing Rust code relied on `Copy` — zero migration burden inside `nros-node`. C/C++ ABI unaffected (both sides see integer codes). The `nros_get_last_backend_error_message` FFI helper is deferred: no backend currently emits a `Backend`/`BackendDynamic` variant, so there's no string to retrieve; add it once a real backend populates the diagnostic.

### Group F — Platform: duplication, CycleCounter units, trait contract

- [x] 84.F1 — Dedupe `net.rs` across 4 bare-metal platform crates via a `nros_smoltcp::define_smoltcp_platform!(PlatformZst)` macro. Each platform's 502-line `net.rs` collapses to a single 8-line file that invokes the macro; the body lives once in `nros-smoltcp::platform_macro`. Net result: 2008 lines → 577 lines (~70% reduction). The macro emits the same five `impl crate::PlatformZst { ... }` blocks (TCP / UDP / socket helpers / multicast stubs) so the existing `zpico-platform-shim` inherent-method dispatch keeps working unchanged. Took the macro approach instead of the audit's "extract a `zpico-smoltcp-platform` crate" because `nros-smoltcp` is already a dep of every bare-metal platform crate, so a new crate would have been pure overhead.
- [x] 84.F2 — Extract `nros-baremetal-common` crate for the shared `xorshift32` RNG (`random.rs`, 70 lines × 4), `sleep.rs` (44 lines × 4), and libc stubs (`libc_stubs.rs`, 247 lines × 2). New crate at `packages/drivers/nros-baremetal-common/`. Each platform crate's `random.rs` becomes a 5-line `pub use`; `sleep.rs` becomes a 12-line wrapper that registers the platform's `clock::clock_ms` via `set_clock_fn` (the shared sleep module uses a function-pointer atomic to call the right clock); `libc_stubs.rs` becomes a 4-8 line note (the actual `#[unsafe(no_mangle)]` symbols are gated behind a `libc-stubs` Cargo feature in `nros-baremetal-common`, enabled only by MPS2-AN385 and STM32F4 — ESP32 / ESP32-QEMU don't enable it because esp-hal already provides them). Net savings: 950 duplicated lines → 511 lines (~46% reduction).
- [x] 84.F3 — Split timing API into **`MonotonicClock::now() -> core::time::Duration`** (portable, available on every platform) and **`CycleCounter`** (raw u32 cycles, only on platforms with real hardware cycle counters — MPS2/STM32F4 via DWT). ESP32 and ESP32-QEMU dropped the fake `CycleCounter` and keep only `MonotonicClock`. Board-crate re-exports updated (`nros-board-mps2-an385` / `nros-board-stm32f4` re-export both; `nros-board-esp32` / `nros-board-esp32-qemu` re-export only `MonotonicClock`). No in-repo example uses `CycleCounter::measure()` so no example migration was needed. Platform-api / custom-platform docs will gain content in a follow-up (the split is documented inline in each platform's `timing.rs`).
- [ ] 84.F4 — Platform traits in `packages/core/nros-platform/src/traits.rs`
      become a real contract: actually `impl PlatformClock`,
      `PlatformAlloc`, `PlatformUdp`, … on each platform ZST and have
      `zpico-platform-shim` / `xrce-platform-shim` dispatch via
      `<P as PlatformClock>::clock_ms()` rather than inherent methods.
      Renaming / adding a trait method now produces a compile error at
      every platform instead of silent link failure. Expect churn
      across all ~9 platform crates; do one crate at a time.

      Current reality (audited 2026-04-24): **zero** `impl PlatformX for _`
      blocks exist in the tree. Every platform uses inherent methods;
      the traits in `nros-platform/src/traits.rs` are documentation-only.
      Shims do name-based dispatch (`P::clock_ms()`, 66 such call sites
      in `zpico-platform-shim/src/shim.rs`). Renaming a trait method
      today silently breaks nothing at compile time.

      Trait ↔ inherent naming audit: ~60 % of names align, ~35 % need
      one side renamed (`PlatformTcp::open` vs `P::tcp_open`;
      `PlatformUdp::open` vs `P::udp_open`;
      `PlatformSocketHelpers::set_non_blocking` vs
      `P::socket_set_non_blocking`), and `PlatformTime::time_since_epoch()
      -> TimeSinceEpoch` has a shape mismatch vs the inherent's two
      separate `time_since_epoch_secs()` / `time_since_epoch_nanos()`
      functions used by the shim.

      Three decisions pinned before migration starts (2026-04-24):

      1. **Drop `tcp_` / `udp_` / `socket_` prefixes from inherent
         method names.** The trait name already namespaces the
         method; `<P as PlatformTcp>::open(...)` reads naturally. ~40
         shim call sites need rewriting; ~45 inherent methods need
         renaming.
      2. **Change `PlatformTime` to match the shipped inherents.**
         Replace `time_since_epoch() -> TimeSinceEpoch` with
         `time_since_epoch_secs() -> u32` and
         `time_since_epoch_nanos() -> u32`. The struct form isn't
         used anywhere; current shim calls the two-function form.
      3. **Bare-metal platforms implement `PlatformThreading` with
         success-returning nops.** Same behaviour as today; avoids the
         bigger refactor of splitting the trait into
         `PlatformMutex` / `PlatformCondvar` / `PlatformTask`
         (deferred to Phase 80 or later).

      Sub-PR sequence:

      - [x] **F4.1 — freeze trait definitions**. Updated
            `packages/core/nros-platform/src/traits.rs`:
            (a) replaced `PlatformTime::time_since_epoch() ->
            TimeSinceEpoch` with `time_since_epoch_secs() -> u32`
            and `time_since_epoch_nanos() -> u32`; removed the
            `TimeSinceEpoch` struct (unused outside the shim;
            `zpico-platform-shim::ZTimeSinceEpoch` is its own FFI
            struct and stays);
            (b) added a module-level naming-convention note
            documenting the "unprefixed methods, dispatch via
            qualified path" rule and the three sub-namespace
            exceptions (`PlatformThreading`'s `mutex_*` / `condvar_*`
            / `task_*`; `PlatformUdpMulticast`'s `mcast_*`; the two
            `close` methods on `PlatformTcp` vs
            `PlatformSocketHelpers`);
            (c) expanded docstrings on `PlatformTcp` and
            `PlatformSocketHelpers` to call out the dispatch
            pattern. `TimeSinceEpoch` removal had one caller
            (`_z_get_time_since_epoch` in zpico-platform-shim) —
            switched to writing `secs` / `nanos` directly to the
            output struct, no behaviour change. No platform crates
            touched — the traits remain unused until F4.2 starts
            wiring them. Workspace compile clean; 14 XRCE E2E
            tests pass post-change.
      - [x] **F4.2 — implement `PlatformClock` + `PlatformAlloc` on
            every platform**. Landed alongside an **extraction PR**
            that couldn't be avoided: the traits were moved from
            `nros-platform` into a new `nros-platform-api` crate so
            platform crates can implement them without creating a
            dep cycle back through `nros-platform`. `nros-platform`
            now re-exports everything from `nros-platform-api`
            unchanged, so `use nros_platform::PlatformClock;`
            callers keep working. All 10 platform ZSTs implement
            `PlatformClock` + `PlatformAlloc` (9 ports plus
            `nros-platform-cffi`'s vtable-backed variant); both
            `zpico-platform-shim` and `xrce-platform-shim` dispatch
            via `<P as PlatformClock>::clock_ms()` / `<P as
            PlatformAlloc>::alloc(...)` etc. Pilot validated the
            approach — 14 XRCE E2E tests + workspace compile pass.
      - [x] **F4.3 — implement `PlatformSleep` + `PlatformRandom` +
            `PlatformTime`**. All 10 platform ZSTs now implement
            these three traits. zpico-platform-shim dispatches via
            qualified paths (`<P as PlatformSleep>::sleep_us(...)`,
            etc.). XRCE shim doesn't call these and is unchanged.
            Workspace compile + 14 XRCE E2E + native zenoh tests
            pass.
      - [x] **F4.4 — implement network traits**
            (`PlatformTcp`, `PlatformUdp`, `PlatformSocketHelpers`,
            `PlatformUdpMulticast`). All 9 networking platforms
            implement these via delegation — inherent methods on
            the platform ZSTs kept as implementation detail, trait
            impls at the bottom of each `net.rs` forward to the
            inherent methods. `zpico-platform-shim` dispatches
            through `<P as PlatformTcp>::open(...)` etc. (method
            names dropped the `tcp_` / `udp_` / `socket_` prefixes
            per the F4.1 naming convention; multicast kept its
            `mcast_*` prefix). Bare-metal platforms get their trait
            impls through `nros_smoltcp::define_smoltcp_platform!`
            which was rewritten to emit trait impls directly.
            `nros-smoltcp` now depends on `nros-platform-api` and
            re-exports the four network traits so the macro can
            reach them via `$crate::PlatformTcp` etc.
            `PlatformNetworkPoll` is documentary only — bare-metal
            platforms call `SmoltcpBridge::poll_network()` inline
            from TCP/UDP send/recv bodies.
      - [x] **F4.5 — implement `PlatformThreading`**. All 10 platform
            ZSTs implement the 22-method trait via delegation to
            their pre-existing inherent methods. POSIX / Zephyr /
            FreeRTOS / NuttX / ThreadX have real threading; bare-
            metal platforms (mps2-an385, stm32f4, esp32, esp32-qemu)
            have success-returning nops; cffi dispatches via the C
            vtable. Trait signatures reconciled with shipped
            inherents: the draft `TaskHandle` / `MutexHandle` /
            `CondvarHandle` typed-opaque wrappers were dropped in
            favour of `*mut c_void` (every shipped impl already
            used `*mut c_void` internally; typed wrappers would
            have required a pointless cast at every impl site).
            `zpico-platform-shim` now dispatches all 22 threading
            entry points via `<P as PlatformThreading>::*`.
      - [x] **F4.6 — `PlatformLibc` + `PlatformNetworkPoll` marked
            documentary**. Audit found these traits are never
            dispatched through by either shim — libc stubs are
            resolved at link time from
            `nros-baremetal-common`'s `#[unsafe(no_mangle)] extern
            "C"` symbols (controlled by its `libc-stubs` feature);
            network polling is called inline from TCP/UDP trait
            bodies. Rather than add dead trait impls on every
            bare-metal platform, the traits remain in the API
            surface with a docstring explaining the link-level
            dispatch and noting that a future shim refactor could
            wire them through. Zero platform crates implement
            these traits today — deliberately.

      Acceptance (per sub-PR): `just ci` green + relevant platform
      tests pass. Acceptance (whole F4): `grep -rn "P::[a-z_]*(" shim`
      returns **zero** non-trait-qualified dispatch calls in
      `zpico-platform-shim` and `xrce-platform-shim`. **Verified
      2026-04-24**: only one residual `P::` is in a doc comment.
- [x] 84.F5 — Landed via a new `nros_smoltcp::NetworkState<D>` generic holder. The struct keeps the `(Interface, SocketSet, Device)` triple in `AtomicPtr` fields (no more `static mut`), exposes `set` / `clear` / `poll` / `poll_via_ref` methods (the `_via_ref` variant covers STM32F4's `for<'a> &'a mut EthernetDMA: Device` quirk), and is `unsafe impl Sync`. Each board's `network.rs` is now ~35 lines of wiring instead of ~63 lines of hand-rolled globals. Board total 254 → 148 lines (-42%), and the 12 `static mut` globals are gone. Board-side net lines (`boards/nros-platform-*/src/net.rs`) were already covered by the `define_smoltcp_platform!` macro in Phase 83.
- [x] 84.F6 — **Landed 2026-04-27**: directory-layout cleanup + board-crate rename. Target layout:
      - `packages/platforms/` — OS-level platform crates (`posix`, `freertos`, `nuttx`, `threadx`, `zephyr`) and bare-metal platform crates (current `packages/boards/nros-platform-*` move here, keeping the `nros-platform-*` name since these are implementer-facing).
      - `packages/boards/` — **user-facing** board bring-up crates. Rename from `nros-*` / `nros-*-freertos` etc. to a clearer prefix like `nros-board-*` (e.g. `nros-board-mps2-an385`, `nros-board-esp32-wifi`). These are the end-user library surface; the rename distinguishes "what I `use` from my app" from "what I implement when porting".
      - Update CLAUDE.md workspace layout and all book porting docs.
      - Sequencing: this is the **last** item in Phase 84 because the rename touches every example Cargo.toml, every board-specific doc, and every `zephyr/modules.yaml`-style integration point. Land it after all other Phase 84 groups stabilize.

      Sub-items (land in this order; each is a self-contained PR):

      - [x] **84.F6.1 — Move `packages/boards/nros-platform-*` to `packages/platforms/`.** Files: top-level `Cargo.toml` workspace `members`/`exclude` lists, every internal `path = "../boards/nros-platform-*"` reference (search `grep -rn 'boards/nros-platform-' packages/ examples/ zephyr/`), `zephyr/modules.yaml`, `zephyr/CMakeLists.txt` if it references board paths, and any `cargo-nano-ros` template that points at the old path. No crate names change, only paths. Acceptance: `just check` + `just test-unit` green; `grep -rn 'boards/nros-platform-' packages/ examples/` returns nothing.

      - [x] **84.F6.2 — Rename user-facing board crates `nros-* → nros-board-*`.** Concrete renames:

            | Old crate name              | Old path                                | New crate name                  | New path                              |
            |-----------------------------|------------------------------------------|---------------------------------|----------------------------------------|
            | `nros-board-mps2-an385`           | `packages/boards/nros-board-mps2-an385`         | `nros-board-mps2-an385`         | `packages/boards/nros-board-mps2-an385` |
            | `nros-board-mps2-an385-freertos`  | `packages/boards/nros-board-mps2-an385-freertos`| `nros-board-mps2-an385-freertos`| `packages/boards/nros-board-mps2-an385-freertos` |
            | `nros-board-stm32f4`              | `packages/boards/nros-board-stm32f4`            | `nros-board-stm32f4`            | `packages/boards/nros-board-stm32f4`   |
            | `nros-board-esp32`                | `packages/boards/nros-board-esp32`              | `nros-board-esp32`              | `packages/boards/nros-board-esp32`     |
            | `nros-board-esp32-qemu`           | `packages/boards/nros-board-esp32-qemu`         | `nros-board-esp32-qemu`         | `packages/boards/nros-board-esp32-qemu`|
            | `nros-board-nuttx-qemu-arm`       | `packages/boards/nros-board-nuttx-qemu-arm`     | `nros-board-nuttx-qemu-arm`     | `packages/boards/nros-board-nuttx-qemu-arm` |
            | `nros-board-threadx-linux`        | `packages/boards/nros-board-threadx-linux`      | `nros-board-threadx-linux`      | `packages/boards/nros-board-threadx-linux` |
            | `nros-board-threadx-qemu-riscv64` | `packages/boards/nros-board-threadx-qemu-riscv64`| `nros-board-threadx-qemu-riscv64`| `packages/boards/nros-board-threadx-qemu-riscv64` |

            `mps2-an385-pac` (the auto-generated PAC) is **not** renamed — it's a one-off vendor crate, not a board bring-up surface.

            Per-crate updates: `Cargo.toml [package].name`, `lib.name`, all `path = "../boards/nros-*"` deps in workspace, top-level workspace `members`/`exclude` arrays, and any `extern crate nros_*` / `use nros_*::` references in `src/`.

            Acceptance: `cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name' | grep -E '^nros-(mps2-an385|stm32f4|esp32|esp32-qemu|nuttx-qemu-arm|threadx-)'` returns nothing; new `nros-board-*` names appear. `just check` green.

      - [x] **84.F6.3 — Update every `examples/**/Cargo.toml` and `examples/**/.cargo/config.toml`.** Patterns to migrate:
            - `[dependencies] nros-board-mps2-an385 = { … }` → `nros-board-mps2-an385 = { … }` (and the same for each board listed in 84.F6.2)
            - `[patch.crates-io] nros-* = { path = "..." }` entries
            - Any `extern crate nros_board_mps2_an385;` / `use nros_board_mps2_an385::` in `examples/**/src/`
            - `examples/*/cargo-template/` files used by `cargo nano-ros init`

            Acceptance: `just build-test-fixtures` succeeds with zero changes to other parts of the recipe.

      - [x] **84.F6.4 — Update book + porting docs.** Files (start with): `book/src/getting-started/{bare-metal,freertos,nuttx,threadx,esp32,stm32f4}.md`, `book/src/porting/{custom-board,custom-platform}.md`, `book/src/reference/platform-api.md`, `book/src/concepts/platform-model.md`, `book/src/internals/creating-examples.md`, every `docs/research/` and `docs/design/` reference. Search: `grep -rn 'nros-board-mps2-an385\|nros-board-stm32f4\|nros-board-esp32\|nros-board-nuttx-qemu-arm\|nros-threadx-' book/ docs/`. Acceptance: `just book` builds clean; spot-check a "Getting Started" page builds an example with the new crate name.

      - [x] **84.F6.5 — Update CLAUDE.md workspace layout block** (the ASCII tree under "Workspace Structure") and any `Boards:` / `Platforms:` lines in the Phase summary table or design pattern sections.

      - [x] **84.F6.6 — Update Zephyr module wiring + cmake.** Files: `zephyr/modules.yaml`, `zephyr/cmake/*.cmake`, `examples/zephyr/**/CMakeLists.txt` if any reference the board crate. The Zephyr workspace patches in `scripts/zephyr/` may also embed crate names — sweep those too. Acceptance: `just zephyr build-fixtures` clean rebuild from a wiped workspace.

      Phase-level acceptance for 84.F6 as a whole:

      - `grep -rEn '\bnros-(mps2-an385|stm32f4|esp32|esp32-qemu|nuttx-qemu-arm|threadx-linux|threadx-qemu-riscv64)\b' packages/ examples/ book/ docs/ CLAUDE.md zephyr/` returns **zero hits outside intentionally-pinned migration notes**.
      - `packages/platforms/` exists and is non-empty; `packages/boards/nros-platform-*` is gone.
      - `just ci` + `just test-all` both green.
- [x] 84.F7 — `Config` per-board divergence: defined `nros_platform::BoardConfig { zenoh_locator(&self) -> &str, domain_id(&self) -> u32 }` and implemented it on all 4 board configs (`nros-board-mps2-an385::Config`, `nros-board-stm32f4::Config`, `nros-board-esp32::NodeConfig`, `nros-board-esp32-qemu::Config`). Cross-board generic code can now read the universal fields via `&impl BoardConfig` instead of `cfg`-gating on each board type. Each board's transport-specific knobs (MAC, IP, WiFi credentials, UART base) stay on the concrete struct as ordinary fields. Trait re-exported from each board crate's root + prelude. `from_toml` was NOT added to the trait — every board's parser keeps its own implementation since the TOML schemas legitimately differ; unifying them is a separate (larger) refactor.
- [x] 84.F8 — Move `_z_listen_udp_unicast` from a hard-coded `-1` stub in `zpico-platform-shim::shim.rs:503` onto `PlatformUdp::udp_listen(...)` with a default `-1` impl so future ports can override *(trait default added; shim stays `-1` until Phase 84.F4 switches platforms to trait dispatch)*

### Group G — Minor / nit

Small wins worth rolling into the PRs of adjacent groups.

- [x] 84.G1 — C API: drop `_init_default` / `_init_best_effort` aliases on publisher/subscription; standardize on `_init` + `_init_with_qos`
- [x] 84.G2 — C API: standardize `_is_valid` predicates to `bool` (currently a mix of `int` and `bool`)
- [x] 84.G3 — **Resolved by audit + delete** (2026-04-21). The original framing assumed `nros_subscription_t._internal` and `nros_node_t._internal` were heap-allocated via `Box::leak`, matching publisher/executor/service/action's inline opaque-storage pattern. Audit says otherwise: both fields are always `ptr::null_mut()` at init, never written to anything non-null, and never read — `nros_node_init` explicitly says "we don't create an internal Rust node object," and subscription's init defers to the executor arena. Rather than invent inline storage for zero bytes of real state, deleted the fields outright (plus null-initialisers, null-resetters, is_null checks, a dead `get_internal()` helper, and a Kani-harness assertion). Commit `f102d38b` — 4 files, -26/+2 lines. Source-compatible for well-behaved consumers; struct size shrinks by `sizeof(void*)`.
- [x] 84.G4 — C API: fix `#include <nano_ros/init.h>` copy-paste bug in `nros/platform.h:15` (should be `<nros/init.h>`)
- [x] 84.G5 — C API: add missing array-parameter function declarations to `parameter.h`. The Rust implementations already exist via the `impl_param_array!` macro (byte/bool/integer/double/string variants, declare/get/set), but were never exposed in any C header — making the `NROS_PARAMETER_*_ARRAY` enum values effectively unreachable. 15 function declarations added; no Rust changes needed.
- [x] 84.G6 — C API: `nros_executor_trigger_one` now reads `*(size_t*)context` instead of casting `context as usize`. Callers point at a real `size_t`, which is typed and UB-free on CHERI / strict-alignment targets.
- [x] 84.G7 — **Landed** (2026-04-22). `ActionClient<A>::feedback_stream()` returns `Stream<FeedbackType>&`, mirroring `Subscription<M>::stream()` — lazy-bound on first call, rebound on move (Phase 84.C1 relocation), wraps the existing `nros_cpp_action_client_try_recv_feedback` FFI so no new arena plumbing was needed (Phase 77 already unified the feedback path through a single arena core). Feedback is not goal-scoped at this layer; the stream yields `FeedbackType` values across all active goals for the client, matching `try_recv_feedback`. Callers that need per-goal separation should use the `SendGoalOptions::feedback` callback, which receives `(goal_id, bytes, len, ctx)`. `examples/native/cpp/zenoh/action-client` updated to demonstrate the new shape. Commit `15540374` — 2 files, +43/-2.
- [x] 84.G8 — **Landed** (2026-04-24). `node.hpp` now carries only the forward declarations and member-template *declarations* of the six heavy `create_X<T>()` entries (publisher, subscription, service, client, action_server, action_client). Each entity header (`publisher.hpp`, `subscription.hpp`, …) provides the corresponding out-of-line `template <typename T> Result Node::create_X(...)` *definition* after its own class body, guarded by `#include "nros/node.hpp"`. Non-templated `create_timer` / `create_timer_oneshot` / `create_guard_condition` stay inline in `node.hpp` because their light entity headers (timer.hpp, guard_condition.hpp) were already cheap. The umbrella `nros.hpp` now pulls in every entity header explicitly so `#include <nros/nros.hpp>` still gives the full API. Verified: consumers that include only `nros/node.hpp` compile to a 648-byte object (no template instantiations pulled in from entity code); `test_cpp_action_communication`, `test_cpp_action_goal_rejection`, `test_cpp_rust_pubsub_interop`, `test_cpp_rust_service_interop`, and both `::lang_2_Language__Cpp` variants of `test_native_talker_listener_communication` / `test_native_service_communication` still pass.
- [x] 84.G9 — **Landed** (2026-04-22). Added `ActionServer<A>::set_goal_callback_with_ctx(TypedGoalFnWithCtx, void*)` plus the parallel `set_cancel_callback_with_ctx(TypedCancelFnWithCtx, void*)` — the standard "fn + `void*` context" escape hatch used by every other C callback boundary in the project. Each slot (goal / cancel) has two mutually-exclusive modes: setting one variant clears the other; trampolines prefer the `_with_ctx` slot when set. The existing `set_goal_callback<F>(F)` templated overload with its stateless-callable `static_assert` (from 84.C6) is unchanged. `examples/native/cpp/zenoh/action-server/main.cpp` migrated to `set_goal_callback_with_ctx` + a stack-allocated `ServerState` to demonstrate global-free callback state and exercise the new trampoline at runtime in `test_native_action_server_starts::Cpp`. Commit `45767b22` — 2 files, +97/-25.

## Acceptance Criteria

Per-group, all items in the group's checklist are either checked off or
explicitly deferred with a rationale in Notes. No blanket phase-wide
acceptance — each group may land and be verified independently.

Cross-cutting criteria that apply once all groups land:

- [x] `cargo test --doc -p nros` passes in CI (Group A). Verified 2026-04-20: 1 passed, 4 ignored, 0 failed.
- [x] `grep -rn 'ZENOH_LOCATOR' book/ packages/` returns only legacy-fallback
      / migration-doc hits (Group E).
- [x] No `static mut` in `packages/xrce/nros-rmw-xrce/src/lib.rs` session /
      transport globals (Group E). Delivered by 84.E2b.1 (module
      attribute `#![allow(static_mut_refs)]` also removed).
- [x] `wc -l packages/boards/nros-platform-*/src/{net,random,sleep,libc_stubs}.rs`
      drops by ≥70% (Group F). **Verified 2026-04-24**: 483 lines across
      16 files, down from the pre-Phase-83 baseline of ~2000+ lines per-file
      `net.rs` × 4 platforms plus duplicated random/sleep/libc_stubs. Delivered
      by Phase 83's `define_smoltcp_platform!` macro + `nros-baremetal-common`
      dedupe (84.F1 / 84.F2).
- [x] `nros-c/src/{cdr,parameter,lifecycle}.rs` combined line count **does
      not grow** from pre-Phase-84 (Group B — target revised 2026-04-24).
      **Current**: 3065 → 2557 lines (cdr 617 / parameter 1322 / lifecycle
      618), a 17% reduction. The original ≥60% target set in the phase
      doc was unreachable because B3 and B4 both intentionally kept the
      legacy C FFI surfaces (`nros_param_server_t` storage path, lifecycle
      C FFI) for back-compat; the real wins from those items are
      architectural (delegation to `nros-node` and `nros-lifecycle-msgs`),
      not line-count. Revised criterion reflects the actual goal —
      "don't let this drift back up" — and is met.

## Open Questions

**All eleven open questions are now resolved.** Resolutions are spec'd
directly in Work Items above.

Historical record of each question and the trade-off analysis that
shaped the final plan is preserved below for future readers — both to
document *why* we picked what we picked and to give a landing spot for
new questions that relate to the same areas.

### Resolved — OQ-1 — C++ opaque-storage move via a Rust-side bridge (84.C1)

**User direction**: study if we can bridge to Rust to implement C++ move.

**Why a Rust bridge works**: In Rust, every non-`Pin` struct is bitwise
movable by definition — `memcpy` of the bytes *is* a valid move as long as
**nothing outside the struct holds a pointer back to its old address**.
The unsafety in nros-cpp today isn't Rust's object invariant; it's that
the Rust type sometimes registers its own storage address with the
Executor arena (as the callback `ctx`) and the arena still points at the
old address after C++ `memcpy`.

So the fix is surgical: per-type, Rust exposes an FFI `*_relocate(old,
new)` that re-registers whatever external consumers hold the old pointer.
C++'s move ctor calls it.

**Pattern (per opaque-storage type)**:

```rust
// packages/core/nros-cpp/src/lib.rs
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_relocate(
    old: *mut CppPublisher,
    new: *mut CppPublisher,
) -> nros_cpp_ret_t {
    // Move the Rust value bitwise (safe: CppPublisher is not Pin).
    let value = unsafe { core::ptr::read(old) };
    unsafe { core::ptr::write(new, value) };
    // CppPublisher registers nothing external with its storage address,
    // so nothing else to patch. Return OK.
    NROS_CPP_RET_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_relocate(
    old: *mut CppGuardCondition,
    new: *mut CppGuardCondition,
) -> nros_cpp_ret_t {
    let value = unsafe { core::ptr::read(old) };
    unsafe { core::ptr::write(new, value) };
    // GuardCondition registered `old` as callback ctx with the executor.
    // Re-register with `new`.
    unsafe { (*new).reregister_callback(new as *mut c_void) };
    NROS_CPP_RET_OK
}
```

```cpp
// packages/core/nros-cpp/include/nros/publisher.hpp
template <typename M>
class Publisher {
public:
    Publisher(Publisher&& other) noexcept {
        if (other.initialized_) {
            nros_cpp_publisher_relocate(
                reinterpret_cast<void*>(other.storage_),
                reinterpret_cast<void*>(storage_));
            initialized_ = true;
            other.initialized_ = false;
        }
    }
    Publisher& operator=(Publisher&& other) noexcept {
        if (this != &other) {
            if (initialized_) nros_cpp_publisher_destroy(storage_);
            if (other.initialized_) {
                nros_cpp_publisher_relocate(
                    reinterpret_cast<void*>(other.storage_),
                    reinterpret_cast<void*>(storage_));
                initialized_ = true;
                other.initialized_ = false;
            } else {
                initialized_ = false;
            }
        }
        return *this;
    }
};
```

**Per-type audit needed** (list every opaque-storage type; mark what it
registers externally with its storage address):

| Type | Registers old ptr with … | Relocate patches |
|------|--------------------------|-----------------|
| `Publisher` | nothing (stateless FFI) | just `ptr::read`/`write` |
| `Subscription` | arena callback (`ctx = storage`) | re-register ctx |
| `Service` | arena callback | re-register ctx |
| `Client` | single-slot promise ptr | re-register slot |
| `GuardCondition` | user cb ctx | re-register cb |
| `ActionServer` | goal/cancel/accept trampolines | already does this |
| `ActionClient` | goal response / feedback / result cbs | re-register |
| `Executor` | session owner | session re-bind |
| `Timer` | arena timer slot ctx | re-register ctx |

**Benefits**:
- Moves stay no-heap.
- UB is confined to the per-type `_relocate` — a single audited function
  per type that future contributors can't forget (compile-time enforced
  by requiring every type to implement a `CppRelocate` trait).
- The C++ side is uniform — one `memcpy`-free move ctor pattern.

**Resolution**: user chose per-entity `handle.reregister(...)` on
`nros-node`'s raw handle types. `nros-cpp`'s `_relocate` FFI stays thin
(`ptr::read`/`write` + call `handle.reregister`); `nros-node` stays free
of C++ artifacts. See 84.C1.

### Resolved — OQ-3 — `CycleCounter` unit contract (84.F3)

**Current code** (real, not hypothetical):

```rust
// packages/boards/nros-platform-mps2-an385/src/timing.rs
impl CycleCounter {
    pub fn enable() { /* DWT init */ }
    pub fn read() -> u32 { cortex_m::peripheral::DWT::cycle_count() }  // cycles
    pub fn measure<F, R>(f: F) -> (R, u32) { /* wraps read() */ }
}

// packages/boards/nros-platform-esp32/src/timing.rs
impl CycleCounter {
    pub fn enable() {}  // no-op
    pub fn read() -> u32 { Instant::now().duration_since_epoch().as_micros() as u32 }  // µs
    pub fn measure<F, R>(f: F) -> (R, u32) { /* wraps read() */ }
}

// packages/boards/nros-platform-stm32f4/src/timing.rs
// Identical to mps2-an385 — cycles.
```

Same trait surface, three different units. Portable benchmark code
silently reports wrong numbers.

**Option A — all cycles + `cycles_per_us()`**:

```rust
pub trait CycleCounter {
    fn enable();
    fn read() -> u32;                    // always cycles
    fn cycles_per_us() -> u32;           // e.g. 168 on STM32F4 @168MHz
    fn measure<F, R>(f: F) -> (R, u32);  // cycles
}
// user code:
let (_, cycles) = CycleCounter::measure(|| work());
let us = cycles / CycleCounter::cycles_per_us();
```
Pros: cycle-exact; fast on hardware that has a free-running counter.
Cons: users always convert; ESP32 returns µs natively, so it'd have to
either scale up (useless precision) or expose `cycles_per_us() = 1`
(confusing).

**Option B — all `Duration`**:

```rust
pub trait HighResTimer {
    fn enable();
    fn read() -> core::time::Duration;
    fn measure<F, R>(f: F) -> (R, core::time::Duration);
}
// user code:
let (_, elapsed) = HighResTimer::measure(|| work());
if elapsed > Duration::from_micros(50) { /* warn */ }
```
Pros: portable; reads naturally. Cons: sub-µs resolution lost if the
backing clock is µs (ESP32); cycle-exact users give up the raw u32 they
had.

**Option C — split into `MonotonicClock` + optional `CycleCounter`**:

```rust
pub trait MonotonicClock {
    fn now() -> core::time::Duration;
}
pub trait CycleCounter {              // optional — not every platform provides
    fn enable();
    fn read() -> u32;
    fn cycles_per_us() -> u32;
}
// user code (portable):
let t0 = P::MonotonicClock::now();
work();
let elapsed = P::MonotonicClock::now() - t0;
// user code (cycle-exact, only on platforms that implement the trait):
#[cfg(platform_has_cycle_counter)]
{ let (_, c) = CycleCounter::measure(|| ...); }
```
Pros: portable code always works; cycle-exact code is explicit about its
platform requirement. Cons: two traits instead of one; documentation
surface grows.

**Resolution**: user chose Option C. See 84.F3.

### Resolved — OQ-E2 — `Rmw::open` signature change (84.E2)

**Resolution**: Design A (consume `self`) with `Executor::open(&config)`.
Split into two PRs (84.E2a = behavior-preserving rename + deprecation
shim; 84.E2b = delete XRCE `static mut` + delete the shim once the
refactor is complete). See 84.E2a / 84.E2b / 84.E2c in the Work Items.

The full design is preserved below for reference and for future
contributors porting a third backend.

**Current signature** (`packages/core/nros-rmw/src/traits.rs`):

```rust
pub trait Rmw {
    type Session: Session;
    /// Static: takes no `self`, receives all config through `RmwConfig`.
    fn open(config: &RmwConfig) -> Result<Self::Session, TransportError>;
}
```

**Why it's a problem**:
`Rmw::open` is called at `Executor::open()` time (see
`packages/core/nros-node/src/executor/spin.rs:66-108`). Because `open` is
static, a backend that needs to hold **any** state *before* the session
exists (agent-connection retry state, shared reactor, TLS ctx, config
parsed from a file) has nowhere to put it except module-level statics.
`nros-rmw-xrce` demonstrates the consequence: `src/lib.rs:91-97, 510-603`
uses `static mut TRANSPORT / SESSION / INITIALIZED` which blocks:

- **Parallel tests** (global state is shared across threads).
- **Multi-session** (one process, two executors — can't open a second
  XRCE session).
- **Clean re-entry after `close()`** (re-`open()` must reset the same
  statics; order of operations is fragile).

A third backend (rmw-cyclone, rmw-fastrtps, a test harness mock) would
be forced into the same pattern. Any shared init / teardown logic has to
live either in the backend's `static mut` or in a `static`
`LazyLock`/`OnceCell` — both of which are hard to audit for correctness
in `no_std` + embedded.

**Candidate designs**:

**Design A — instance-method `open(self, …)`**:

```rust
pub trait Rmw: Sized {
    type Session: Session;
    fn open(self, config: &RmwConfig) -> Result<Self::Session, TransportError>;
}

// zenoh impl:
pub struct ZenohRmw;                                  // ZST
impl Rmw for ZenohRmw {
    type Session = ZenohSession;
    fn open(self, config: &RmwConfig) -> Result<…>  { /* current body */ }
}

// xrce impl:
pub struct XrceRmw {
    transport: XrceTransport,                         // state lives here
    agent_addr: [u8; 4],
    agent_port: u16,
}
impl Rmw for XrceRmw {
    type Session = XrceSession;
    fn open(self, config: &RmwConfig) -> Result<…>  {
        // self moves INTO the session; no `static mut` needed.
    }
}

// call site (nros-node::Executor::open):
let backend: ConcreteRmw = ConcreteRmw::default();   // cfg-selected type
let session = backend.open(config)?;
```

Pros: simplest change. Zenoh backend's ZST stays a ZST. XRCE drops every
`static mut`. Backends that need config-at-construction have it naturally.

Cons: signature break. Anyone implementing `Rmw` on their own backend
(including tests using a mock) must rename `fn open(config)` →
`fn open(self, config)`. `nros-node` must construct the backend value
before calling `open` — the current feature-based type alias
(`ConcreteRmw = ZenohRmw` or `XrceRmw`) needs a companion
`ConcreteRmw::default()` or `::new()`.

**Design B — separate `Backend` factory trait**:

```rust
pub trait Backend {
    type Rmw: Rmw;
    fn new() -> Self;
    fn open(&self, config: &RmwConfig) -> Result<<Self::Rmw as Rmw>::Session, TransportError>;
}

pub trait Rmw {
    type Session: Session;
}

// zenoh impl:
pub struct ZenohBackend;                              // ZST
impl Backend for ZenohBackend {
    type Rmw = ZenohRmw;
    fn new() -> Self { ZenohBackend }
    fn open(&self, c: &RmwConfig) -> Result<…>  { /* current body */ }
}

// xrce impl:
pub struct XrceBackend { /* state */ }
impl Backend for XrceBackend {
    type Rmw = XrceRmw;
    fn new() -> Self { XrceBackend { /* parse agent addr, etc. */ } }
    fn open(&self, c: &RmwConfig) -> Result<…>  { /* no static mut */ }
}
```

Pros: `Rmw` itself becomes a pure "here's the session type" marker.
`Backend` is the factory — a clean separation. Supports more than one
session-open per `Backend` instance (useful for test harnesses).

Cons: two traits to explain; one more indirection in `nros-node`. Also
still a signature break at the `nros-rmw` level.

**Design C — `&mut self` on `Rmw::open`**:

```rust
pub trait Rmw {
    type Session: Session;
    fn open(&mut self, config: &RmwConfig) -> Result<Self::Session, TransportError>;
}
```

Pros: minimal change vs current API (add `&mut self`). Backend can store
state in `&mut self` before open is called.

Cons: The backend instance must outlive the session, but nothing in the
trait enforces it — if `Rmw` is dropped while `Session` is alive and
`Session` references it, unsafety. Less clean than A (which consumes
`self`) or B (which makes `Backend` clearly long-lived).

**Sub-questions for decision**:

1. **Design A vs B vs C?** A is simplest; B separates concerns; C is the
   smallest diff.
2. **How is the backend instance constructed in `nros-node`?** Options:
   - (i) `ConcreteRmw::default()` at `Executor::open()` — simple but
     forces every backend to have sensible defaults.
   - (ii) `ExecutorConfig::backend: ConcreteRmw` field — users construct
     explicitly and pass in; opens the door to `Executor::open_with(backend, config)`.
   - (iii) Feature-gated free function `ConcreteRmw::from_env()` — lets
     the backend read env vars at construction (matches how zenoh reads
     `ZENOH_LOCATOR` / `ZENOH_MODE` today via `ExecutorConfig::from_env`).
3. **Deprecation shim?** Can `nros-rmw` keep the old `fn open(config)`
   as a provided default that calls `Self::default().open(config)` for
   one release cycle, so third-party backends can migrate without a
   hard break? Or is a clean break acceptable given we ship two backends?
4. **Does this land in Phase 84 or get its own phase?** The blast radius
   of (1) + (2) is small within the repo — maybe ~300 lines across
   `nros-rmw`, `nros-rmw-zenoh`, `nros-rmw-xrce`, `nros-node`. But it is
   a porting-guide commitment (`book/src/porting/custom-rmw.md` becomes
   prescriptive about factory shape). If a large RMW refactor is
   imminent (e.g., the Phase 80 unified network interface triggers it),
   defer.

**Chosen plan**: Design A + construction option (ii) (users pass
`backend` via `ExecutorConfig`) + `Executor::open(&config)` (borrowing,
`ConcreteRmw: Clone` so reopens are cheap) + **two-PR split**:

- **PR 1 (84.E2a)** — behavior-preserving trait rename, `Default` on
  both backends, `ExecutorConfig.backend` field, `RmwLegacy` deprecation
  shim. XRCE statics stay.
- **PR 2 (84.E2b)** — delete XRCE `static mut`, move transport state
  into `XrceRmw`, verify multi-session. **Delete the deprecation shim**
  at the end of this PR — the refactor is complete and no external
  callers remain in-repo.

Full API design (trait surface, both backend impls, nros-node
integration, user call site, `from_env` handling) is documented in the
Phase 84 design discussion (April 2026). Reproducing the key signature
here for reference:

```rust
pub trait Rmw: Sized {
    type Session: Session;
    type Error:   core::fmt::Debug;
    fn open(self, config: &RmwConfig) -> Result<Self::Session, Self::Error>;
}
```

### Resolved — OQ-10 — `TransportError`: remove `Copy`? C/C++ impact audit (84.E11)

**User direction**: open to removing `Copy` — want to see C/C++ API
effects first.

**Audit — where `TransportError` reaches C/C++**:

The type is defined at `packages/core/nros-rmw/src/traits.rs:200`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    ConnectionFailed, Disconnected, PublisherCreationFailed,
    SubscriberCreationFailed, …, PollFailed, KeepaliveFailed, JoinFailed,
}
```

**C API** (`packages/core/nros-c/src/error.rs`) never exposes
`TransportError` as a type. Errors cross the FFI as `nros_ret_t`
(`c_int`) via a compile-time mapping:
```rust
pub const NROS_RET_ERROR: nros_ret_t = -1;
pub const NROS_RET_PUBLISH_FAILED: nros_ret_t = -10;
pub const NROS_RET_SUBSCRIPTION_FAILED: nros_ret_t = -11;
// …
```
Removing `Copy` from `TransportError` has **zero effect** on the C ABI:
the Rust→int conversion is `match self { X => NROS_RET_X }`, which
doesn't require `Copy`.

**C++ API** (`packages/core/nros-cpp/`) has its own `ErrorCode` enum and
wraps every FFI call's `int32_t` return into `Result`. `TransportError`
is never a field of any C++ type. Again zero ABI effect.

**Rust callers** (`nros-node`, `nros-tests`) — this is where removing
`Copy` costs. Current code:
```rust
match err {
    TransportError::Timeout => …,
    TransportError::PollFailed => …,
    e => Err(NodeError::Transport(e))?,  // `e` currently copied
}
```
After removing `Copy` (keeping `Clone`):
```rust
match err {
    TransportError::Timeout => …,
    TransportError::PollFailed => …,
    ref e => Err(NodeError::Transport(e.clone()))?,  // or borrow
}
```
Roughly a dozen match sites in `nros-node` need `ref` or explicit
`.clone()`. Not a burden.

**Upgrade path if we remove `Copy`**:

```rust
pub enum TransportError {
    Timeout,
    ConnectionFailed,
    // …
    /// Backend-specific context. Static strings are enough for most
    /// zenoh-pico / XRCE codes; `alloc` lets backends format dynamic
    /// diagnostics.
    Backend(&'static str),
    #[cfg(feature = "alloc")]
    BackendDynamic(alloc::string::String),
}
```

`Backend(&'static str)` stays `Copy`-able-if-derived (static strs are
`Copy`) but mixing it with `BackendDynamic(String)` forces the drop of
`Copy`. The C mapping stays the same — either flavour compiles to
`NROS_RET_*`, and an optional `nros_get_last_backend_error_message(char*
buf, size_t cap)` FFI lets C users retrieve the string.

**Alternative if we prefer to keep `Copy`**: `source_code: u32`
errno-sidechannel. Stays `Copy`; carries a backend-defined code; C FFI
exposes it as `nros_get_last_error_source_code() -> u32`.

**Resolution**: user accepted the recommendation. See 84.E11.

## Notes & Caveats

- **Group ordering**: A (docs) and G (nits) can land at any time. B–F have
  independent blast radii and can land in any order except **84.F6 (the
  directory / board-crate rename) lands last** — it touches every example
  Cargo.toml and would repeatedly collide with every other in-flight PR.
- **B is the biggest single cost**: the thin-wrapper violations in parameter
  / lifecycle / CDR are multi-day refactors because `nros-node` must grow
  the real implementation first. Consider prioritizing B3 (parameter server)
  since it also closes the "ROS 2 parameter service endpoints don't work
  from C" correctness gap.
- **Phase 80 overlap**: Group F5 (`set_network_state` globals) will be
  absorbed by Phase 80's unified network interface. Either defer F5 to
  Phase 80, or land F5 as a stepping stone.
- **Phase 77 overlap**: Several Group C items depend on Phase 77 landing
  the async action client (to close `static mut BLOCKING_*` flags in
  `nros-cpp/src/action.rs`). Sequence C after the rest of 77 lands.
- **Audit source**: raw agent outputs are in the April 2026 session
  transcript. Full findings include specific file:line references that are
  too dense to repeat here; drop to the transcript when implementing.
- **Not in scope**: new features, new backends, new platforms. This is
  purely a cleanup of what already exists.
