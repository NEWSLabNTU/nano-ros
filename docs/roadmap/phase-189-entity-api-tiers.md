# Phase 189 — Entity API tiers (convenient + customizable builder)

**Goal.** Collapse the executor's entity-constructor sprawl
(`register_subscription`, `register_subscription_raw`,
`register_subscription_buffered_raw_on`,
`register_subscription_raw_with_qos_sized_on`, `create_publisher` /
`create_publisher_raw`, …) into **two tiers**, like Linux `fork` / `clone`:
a **convenient** surface matching rclcpp/rclrs/rclc (the 90% case) over **one
customizable entity builder** that carries every knob (raw/typed, QoS,
`rx_buffer` size, `MessageInfo`, session, sched-context). Enforce the
[naming policy](../design/entity-api-tiers.md#naming-policy): no
`verb_noun_axis_axis_axis` identifiers — a new axis is one short builder method,
never a longer name.

**Status (2026-05-28).** **M1 + M2 DONE** — the Rust entity builder +
convenient surface landed (M1), and the `register_subscription_*` zoo was
removed with all ~63 callers migrated (M2). **M3 + M4 remain** (C/C++
named-options parity; the M4 sweep folded into M2). Design approved in
([`docs/design/entity-api-tiers.md`](../design/entity-api-tiers.md)); split out
of Phase 172 (2026-05-28). Same precedent as Phase 187 (split from 172 W.5): a
cross-cutting refactor that doesn't belong inside the orchestration follow-ups.

**Priority.** P2 — ergonomics + tech-debt paydown; no MVP capability depends on
it. **But Phase 172 bridge topic-forwarding depends on M1** (it needs the
`.message_info()` + `.session()` knobs; see
[`bridge-topic-forwarding.md`](../design/bridge-topic-forwarding.md)).

**Depends on.** Phase 172.K.5 (`NodeBuilder::session_idx` — the `.session()`
knob's primitive) is landed. The `MessageInfo` attachment plumbing
(`publish_raw_with_attachment` / `try_recv_raw_with_attachment`) exists in
`nros-node`.

## Overview

Why a builder (not just an options struct): some axes are **const-generic**
(`rx_buffer::<N>()` sizes the staging array at the type level — can't be a
runtime field) and typed-vs-generic changes the callback's argument type. The
builder threads the const param + callback type through `build`. The C / C++
mirrors (rclc / rclcpp) keep named-options structs; the Rust builder is the
ergonomic front, all lowering to one core. Migration is additive — the
convenient `create_*` surface stays stable; the `register_*_*_*` zoo deprecates
then is deleted; the generator emits builder calls so generated code reads like
application code.

## Milestones

- [x] **M1 — Rust entity builder + convenient surface (unblocks 172 bridge).**
      **Node-centric** (chosen 2026-05-28): `node.publisher(topic)` /
      `node.subscription(topic)` builders with knobs `.typed::<M>()` /
      `.generic(ty, hash)`, `.qos()`, `.rx_buffer::<N>()`, `.message_info()`,
      `.sched_context()`, `.build()`; convenient `create_publisher` /
      `create_subscription` (+ `create_generic_*`) re-pointed at them (thin
      wrappers), rclrs fluent QoS-on-topic kept. **Borrow model:** since
      subscriptions register into the executor's dispatch arena, the
      callback-capable node handle is a `NodeCtx<'_>` borrowing `&mut Executor`,
      used **one at a time** (entity handles are owned + outlive it — no `Arc`,
      matches the embedded `&mut` model; see
      [`entity-api-tiers.md` §Borrow model](../design/entity-api-tiers.md)). The
      bridge builds the dest publisher on one `NodeCtx` (dropped), then registers
      the source subscription on another.
      *Slice 1 DONE* (`5940a0c4f`): publisher builder on the session-borrowing
      `Node` + permissive `MockSession` QoS. *Slice 2 DONE* (`edae5e01d`):
      `Executor::node_mut(id) -> NodeCtx` + the subscription builder
      (`.typed::<M>()`/`.generic()`/`.qos()`/`.build(cb)`) + convenient
      `create_subscription` / `create_generic_subscription`, delegating to
      `register_subscription_buffered_on`/`_raw_on`. *Slice 3 DONE* — bounded
      knobs `.rx_buffer::<N>()` (typestate `const RX`) + `.sched_context()`
      (`fb7a4d24c`); `.message_info()` (`4d3e794a9`) yielding a
      `GenericSubInfoBuilder` whose `FnMut(&[u8], &RawMessageInfo)` callback
      surfaces the wire attachment (new `nros_core::RawMessageInfo` + flat-buffer
      `SubBufferedRawInfoEntry` + `register_subscription_buffered_raw_info_on`);
      `NodeCtx::publisher` symmetry + convenient `create_publisher`/
      `create_generic_publisher` (`create_publisher_{,raw_}on`). The bridge
      relay (`exec.node_mut(dst).publisher(t).generic(..).build()` then
      `exec.node_mut(src).subscription(t).generic(..).message_info().build(cb)`)
      is verified expressible in `builder_tests::nodectx_publisher_and_bridge_shape`.
- [x] **M2 — Remove the `register_subscription_*` zoo + migrate all callers.**
      **No deprecation window** (decided 2026-05-28): the zoo is an internal
      surface (only the generator + tests/examples + the C FFI call it), so
      delete outright and migrate callers in the same change — a deprecation
      release buys nothing for an unpublished internal API.
      *Generator switched DONE* (codegen `d300164`, superproject `1c7166202`):
      subscriber emission uses the builder
      (`executor.node_mut(n).subscription(t).generic(..).qos(..).rx_buffer::<1024>().build(|_d| {})`),
      dropping `register_subscription_raw_with_qos_sized_on` + the dead
      `noop_raw_subscription` from generated code; verified by `nros-cli-core`
      e2e (generated bare-metal/stm32f4/nuttx/freertos/threadx-riscv64/esp32/
      native fixtures compile+link) + `builder_tests::generator_emitted_chain_compiles`.

      **Callsite inventory (2026-05-28, ~78 sites / 32 files):** 14 Rust examples
      + 6 C examples (via the C FFI wrapper), 4 `packages/testing` files
      (benches + 2 integration tests), ~45 nros-node `executor/tests.rs` unit
      tests, 4 doc-comment examples, 2 C-FFI callsites (`nros-c/src/executor.rs`
      `nros_executor_register_subscription`), 5 internal `node.rs` builder/
      convenience delegations.

      **Surface decisions:**
      - **3 closure cores stay** (the builder's lowering target, made
        `pub(crate)` — not public, not "zoo"): `register_subscription_buffered_on`
        (typed), `_raw_on` (generic), `_raw_info_on` (generic + `RawMessageInfo`).
      - **1 C-FFI core stays** for `nros-c` (closure builder is Rust-only): a
        single clean-named raw fn-ptr+context primitive (rename
        `register_subscription_raw_with_qos_sized{,_on}` → e.g.
        `add_arena_subscription_c_callback` + a node-scoped sibling; one core,
        not the `_raw_with_qos_sized_on` chain). `nros_executor_register_subscription`
        re-points to it.
      - **Builder gains the remaining typed axes** so `with_info`/`with_safety`
        callers migrate: typed `.message_info()` (`FnMut(&M, Option<&MessageInfo>)`,
        used by `examples/native/rust/listener/src/lib.rs`) + `.safety()`
        (`FnMut(&M, &IntegrityStatus)`, `cfg(safety-e2e)`, used by
        `examples/native/rust/listener/src/main.rs`).
      - **~12 public variants deleted** (no external callers / pure internal
        delegation chains): `register_subscription{,_sized,_on,_sized_on}`,
        `register_subscription_buffered{,_raw}`, `register_subscription_with_info{,_sized,_sized_on,_on}`,
        `register_subscription_with_safety{,_sized,_sized_on,_on}`,
        `register_subscription_raw{,_sized}`, `register_subscription_raw_with_qos{,_sized}`.

      **Slices — ALL DONE:** *M2.a* (`650c52b02`) typed `.message_info()` +
      `.safety()` knobs, typed-info/safety `_inner` cores `pub(crate)` +
      qos-threaded. *M2.b+d* (`d72cbe22b`) C-FFI core
      `add_arena_subscription_c_callback` (`pub` — cross-crate) + `nros-c`
      repoint. *M2.c+e* (`9e99c9cb4`) migrated every caller (16 examples, ~45
      unit tests, 2 benches, 2 integration tests, 5 doc comments) + deleted the
      19 public zoo methods + repaired intra-doc links.

      **Kept cores (final):** 3 closure lowering targets
      (`register_subscription_buffered_on` / `_raw_on` / `_raw_info_on`, now
      `pub(crate)`), the typed info/safety `_inner` cores (`pub(crate)`),
      `add_arena_subscription_callback` (`pub` — `nros-px4` external), and
      `add_arena_subscription_c_callback` (`pub` — `nros-c` FFI). No
      `verb_noun_axis_axis_axis` public identifier remains.

      **Verified:** `cargo doc -D warnings` clean; nros-node 145/146 tests
      (default/safety-e2e); nros-c/nros/nros-cpp build; native examples build;
      9 embedded examples cross-compile (baremetal{,-serial}, freertos
      listener+talker, nuttx, threadx-riscv64, threadx-linux, esp32,
      esp32-qemu-baremetal). Zephyr Rust example: source migrated (identical
      builder pattern) but its `just zephyr build-rust-examples` recipe has a
      pre-existing shell-syntax bug — verify once that's fixed.
- [ ] **M3 — C / C++ named-options parity (rclc / rclcpp mirrors).** Rust gets
      the builder; C/C++ keep idiomatic **named-options structs** alongside the
      existing `QoS` arg (rclcpp `create_subscription<M>(topic, qos, cb,
      options)` — qos stays separate, `options` carries the non-QoS axes).
      Survey (2026-05-28): both bindings already pass `QoS`
      (`nros::QoS` / `nros_qos_t`); the `NodeBuilder` / `nros_node_options_t`
      pair is the existing options precedent; **C services/actions take no QoS at
      all** (parity gap). Axis map — `.qos()` = existing arg; `.sched_context()`
      = cheap option field (`nros_{,cpp_}executor_bind_handle_to_sched_context`
      already exists → create-then-bind); `.message_info()` = needs a *new*
      C-fn-ptr-with-info arena path (none today); `.rx_buffer::<N>()` =
      compile-time const in C/C++, not a runtime field (reserved).

      **Slices:**
  - [x] **M3.1 — C++ Pub/Sub options DONE.** `nros::SubscriptionOptions` /
        `nros::PublisherOptions` (new `include/nros/options.hpp`) + overloads
        `create_subscription<M>(out, topic, qos, options)` /
        `create_publisher<M>(out, topic, qos, options)`; existing 2/3-arg calls
        preserved. Fields: `sched_context` (+ reserved `message_info`).
        **Finding:** the C++ subscription is a *poll-style thin wrapper* over a
        bare `RmwSubscriber` (`try_recv_raw`), registering **no** executor
        callback entry — so it has no bindable `HandleId`, and `sched_context`
        is a documented inert no-op on this path until a handle-returning create
        FFI lands (M3.4-scale). The option + overload are wired so the
        rclcpp-shaped call site is stable and binding activates transparently
        once the FFI grows a handle. `PublisherOptions` is empty (symmetry).
  - [x] **M3.2 — C Pub/Sub options DONE.** `nros_subscription_options_t` /
        `nros_publisher_options_t` (cbindgen, mirroring `nros_node_options_t`) +
        `nros_{subscription,publisher}_init_with_options` + `*_get_default_options`.
        `sched_context` field is **functional** here: the C subscription
        registers a C-fn-ptr callback (`add_arena_subscription_c_callback`) and
        gets a `handle_id`, so `nros_executor_register_subscription` binds it via
        `bind_handle_to_sched_context` after registration (errors on bad SC id;
        `0` = inherit default). Reserved `message_info`; `nros_publisher_options_t`
        thin. (Divergence from M3.1: C subs are callback-registered → live sched
        bind; C++ subs are poll-style → inert. Reconciling needs the M3.4 FFI.)
  - [x] **M3.2.g — Callback-style C++ subscriptions DONE.** Resolves the M3.1/M3.2
        divergence above (C++ subs were poll-only → inert sched-bind) — the data-plane
        analogue of the M3.3.e/.f service+client callback work.
        `nros::Node::create_subscription<M>(out, topic, callback, qos, options)` now
        registers a callback in the executor arena (rclcpp dispatch model). New FFI
        `nros_cpp_subscription_register` → `Executor::add_arena_subscription_c_callback`
        (the same hook the C subs use), returning a real `HandleId` so
        `options.sched_context` is **functional** for C++ subs too. `Subscription<M>`
        gains a callback mode: `TypedSubscriptionFn` + a static `message_trampoline`
        (`ffi_deserialize` → typed handler) + dtor/move guards (arena owns the entity;
        no destroy/relocate of `storage_`, no move-after-register). Faithful copy of
        the M3.3.e pattern: FFI excluded from cbindgen (external `RawSubscriptionCallback`
        alias) + local fn-ptr typedef in `subscription.hpp`; SFINAE-guarded overload
        keeps the poll-style create unambiguous. Verified: nros-cpp build + clippy
        clean; the callback overload + both poll overloads compile via
        `g++ -fsyntax-only` in C++14 `-fno-exceptions -fno-rtti` **and** C++17
        `-fexceptions -frtti` (Autoware build mode). Eliminates the poll-trampoline
        every ported rclcpp node (e.g. autoware-safety-island's `SubscriptionHandler`)
        writes by hand.
  - [x] **M3.3 — Services / actions QoS + options parity. DONE** (2026-05-29) —
        all sub-items a–f complete: QoS via Phase 193, then sched-context binding
        + named-options structs across C (services/clients/actions) and C++
        (action server + callback-style services/clients), with a runtime
        EDF-dispatch proof (M3.3.d). Every C/C++ service-plane entity is
        arena-registered + sched-bindable. *Original blocker (now resolved):* the Rust core service create
        (`Executor::register_service_raw_sized` → `Session::create_service_server(&ServiceInfo)`)
        takes **no `QosSettings`** — service QoS is fixed at the RMW layer (e.g.
        Cyclone hard-codes RELIABLE+VOLATILE), and the C++ `create_service(qos)`
        arg is currently **discarded** on this path. True parity needs a
        dedicated slice threading `QosSettings` through `ServiceInfo` +
        every backend's `create_service_server` / action create before exposing
        `_with_qos` / `_with_options` in C (and making C++'s qos actually apply).
        The `sched_context` option would also be inert for C++ services (same
        poll-style/no-handle issue as M3.1). Deferred until that core work lands.
        Documented in
        [`docs/design/service-qos-gap.md`](../design/service-qos-gap.md); the
        fix is designed in
        [`docs/design/service-qos.md`](../design/service-qos.md) (upstream
        rclcpp/rclc/rclrs reference + 5-slice breakdown).
        **UPDATE (2026-05-29): the QoS half is now DONE via Phase 193**
        (`QosSettings` threads through `ServiceInfo` + every backend; C/C++
        `create_service(qos)` and the `_with_qos` entry points apply the
        profile to the wire). **What remains in M3.3 = the named-options
        structs only:** C++ `ServiceOptions`/`ActionServerOptions` +
        C `nros_service_options_t`/`nros_action_server_options_t` +
        `nros_*_init_with_options(...)`, mirroring `SubscriptionOptions`.
        Phase 193.3/193.4 explicitly **deferred these here** — their sole
        non-QoS axis is `sched_context`, inert for the C/C++ service/action
        surface today, so the structs must land together with that sched-binding
        substrate, not as empty-reserved scaffolding.

        **Scoped (2026-05-29) — the substrate is wrapper-only, not core.** The
        Rust core already sched-binds every entity uniformly:
        `register_*` returns a `HandleId`, `Executor::bind_handle_to_sched_context(handle, sc_id)`
        attaches it (`spin.rs`), and the C register fns *already capture* that
        HandleId — services/clients store it as `_internal.arena_entry_index = handle_id.0`,
        action servers as `ActionServerRawHandle` (which exposes `.handle_id()`,
        `action.rs:773`). So nothing changes in the core or the RMW layer; the gap
        is purely surfacing the captured handle + a `sched_context_id` input
        through the C/C++ wrappers, mirroring the subscription (Phase 189.M3)
        pattern verbatim (auto-bind block `executor.rs:791–800`,
        `SubscriptionOptions` + `Subscription::sched_handle_id_` /
        `has_sched_handle()` `subscription.hpp:286,330`, the post-create bind in
        `node.hpp:381–399`). Work items:
    - [x] **M3.3.a — C services + clients. DONE** (2026-05-29). `sched_context_id: u8`
          field on `nros_service_t` / `nros_client_t` (default 0); each register's
          `Ok(handle_id)` arm auto-binds when non-zero
          (`bind_handle_to_sched_context(handle_id, SchedContextId(id))` →
          `INVALID_ARGUMENT` on a bad SC), copying the subscription block (the
          slot is captured before the `&mut *self` reborrow to avoid aliasing).
          `nros_service_options_t` / `nros_client_options_t` +
          `nros_{service,client}_get_default_options` +
          `nros_{service,client}_init_with_options` carry a real `sched_context`
          (the M3.3-C structs the QoS work deferred). cbindgen header regenerates
          the symbols; nros-c builds + clippy clean. Kani harnesses
          `service_init_with_options_stashes_sched_context` +
          `client_init_with_options_stashes_sched_context` (run via `just
          verify-kani`). The runtime bind/reject integration test lands with M3.3.d.
    - [x] **M3.3.b — C actions (server + client). DONE** (2026-05-29). Same
          pattern; the server binds `_internal.handle.handle_id()`, the client
          `HandleId(handle.entry_index())`. `sched_context_id` field +
          `nros_action_server_options_t` / `nros_action_client_options_t` +
          `get_default_options` + `init_with_options` (action clients carry no QoS
          → options-only). nros-c builds + clippy clean.
    - [x] **M3.3.c — C++. DONE** (2026-05-29), but the scope is **narrower + more
          honest than first planned**, per the investigation: C++ entities split
          two ways. (1) **Action server = arena-registered** — `nros_cpp_action_server_register`
          → `Executor::register_action_server_raw` gives a real `ActionServerRawHandle`
          whose goal/cancel callbacks are executor-dispatched. So `sched_context` is
          **functional** there: added `ActionServerOptions { sched_context }`
          (`options.hpp`), a 4-arg `create_action_server(out, name, qos, options)`
          overload, and a `sched_context: u8` param on the
          `nros_cpp_action_server_register` FFI that binds `handle.handle_id()`
          internally after register (no handle-surfacing to C++ needed). Verified:
          `examples/native/cpp/action-server` builds + links (zenoh).
          (2) **Services / clients / subscriptions = poll-style** (a bare
          `RmwServiceServer`/`RmwSubscriber` the user drives via `try_recv`) — they
          have **no executor-dispatched callback**, so a `sched_context` is **N/A by
          design**, not inert-unwired. The pre-existing C++ `Subscription` sched
          scaffolding (`sched_handle_id_` hardwired `SIZE_MAX`) confirmed this — it
          never binds. So **no `ServiceOptions`/`ClientOptions` were added**;
          `ActionServerOptions` documents the rationale. Making poll-style C++
          entities bindable = converting them to callback-style (arena-registered)
          C++ services — a separate feature, **not** part of M3.3. (Also fixed the
          M3.3.a/.b service+client register arms: bind must run before the
          `executor as *mut _` store, else `rust_exec`'s `executor._opaque` borrow
          overlaps the whole-executor reborrow — E0499 under the cffi-zenoh feature
          set, missed by the default-feature build.)
    - [x] **M3.3.d — Tests + docs. DONE** (2026-05-29). Compile/link + Kani layer:
          nros-c builds under the cffi-zenoh feature set + clippy clean;
          `examples/native/cpp/action-server` links with the new overload; Kani harnesses
          `{service,client}_init_with_options_stashes_sched_context`. Options structs
          documented inline (`options.hpp`, the C `nros_*_options_t`). **Runtime
          dispatch proof:** `nros-node` test `test_service_dispatch_respects_sched_context`
          — two services bound to EDF sched contexts dispatch in *deadline* order, not
          registration order, in `spin_once` (mirrors `test_edf_dispatch_order` for subs;
          needed a loadable `MockServiceServer`). This shows the service plane rides the
          same SC-ordered dispatch the C/C++ `sched_context` bind drives. (OS-priority
          *thread* routing for services — the `os_pri` worker path — stays with the
          Phase 162 RT harness; the deterministic EDF-ordering proof here doesn't need
          real threads.)
    - [x] **M3.3.e — Callback-style C++ services (arena-registered, sched-bindable). DONE**
          (2026-05-29). The M3.3.c follow-up: gave C++ services the *callback* dispatch the C API
          already has (rclcpp-style), so they live in the executor arena with a real
          `HandleId` — making `sched_context` functional for them too. Mirrors the
          C++ action server's typed-callback→raw-trampoline pattern (freestanding
          C++14 fn-ptrs, ctx = `this`). FFI `nros_cpp_service_server_register`
          (→ `Executor::register_service_raw_sized{,_on}` with a `RawServiceCallback`
          + `out_handle_id` + `sched_context`); `Service<S>` gains a callback mode
          (typed `void(const Request&, Response&)` handler, a static request
          trampoline that de/serializes, `handle_id_` + `callback_mode_` + dtor/move
          guards since the arena owns the entity); `create_service(out, name,
          callback, qos, options)` overload + `ServiceOptions`. The poll-style create
          stays for back-compat. **Constraint:** no-move-after-register (the arena
          holds `this` as the trampoline ctx) — same as the action server.
          **Verified:** nros-cpp builds + clippy clean; `nros_cpp_service_server_register`
          excluded from cbindgen (external-crate `RawServiceCallback` alias) + declared
          locally in `service.hpp`; the callback `create_service` instantiation builds +
          links in `examples/native/cpp/service-server` (temp harness, reverted).
    - [x] **M3.3.f — Callback-style C++ clients (arena-registered, sched-bindable). DONE**
          (2026-05-29). The client analogue of M3.3.e: C++ clients were future/poll-style
          (`send_request` → `Future`); the callback path arena-registers a *response*
          handler so the client owns a real executor handle (response dispatch runs in
          `spin_once`) — making `sched_context` functional. Two FFIs:
          `nros_cpp_service_client_register` (→ `register_service_client_raw_sized{,_on}`
          with a `RawResponseCallback` + `out_handle_id` + `sched_context`; cbindgen-excluded
          + declared locally in `client.hpp`) and `nros_cpp_service_client_send_on_handle`
          (sends on the arena client by handle — clients must *send* as well as receive,
          unlike services; cbindgen-rendered since it takes no callback). `Client<S>` gains
          a callback mode (typed `void(const Response&)` handler, static response
          trampoline, `async_send_request`, `handle_id_`/`callback_mode_` + dtor/move
          guards), a `create_client(out, name, callback, qos, options)` SFINAE overload, and
          `ClientOptions`. Future-style create kept; no-move-after-register constraint.
          Verified: nros-cpp builds + clippy clean; `examples/native/cpp/service-client`
          builds + links (future path intact) and a temp callback-style instantiation
          built + linked (reverted). **With this, every C/C++ service-plane entity
          (services, clients, action servers/clients) is arena-registered + sched-bindable.**
  - [~] **M3.4 — with-attachment subscription path.** **C DONE.** Added
        `SubBufferedRawInfoCEntry` (C-fn-ptr-with-attachment arena entry) +
        dispatch + `Executor::add_arena_subscription_c_info_callback` in
        `nros-node` (flat payload + `RAW_INFO_ATT_CAP` attachment buffers, via
        `try_recv_raw_with_attachment`), the `RawSubscriptionInfoCallback` type,
        and the C FFI `nros_executor_register_subscription_raw_with_info` +
        `nros_subscription_info_callback_t` (cbindgen). Direct-arg form (the
        callback signature differs from `nros_subscription_callback_t`, so it's
        its own entry, not an option flag — the M3.2 `message_info` reserved flag
        is superseded by this dedicated init). Test:
        `tests::test_raw_subscription_info_callback`.
  - [x] **M3.4b — C++ poll-with-attachment DONE.** C++ subscriptions are
        poll-style, so the attachment is surfaced on the poll path (not a
        callback): `Subscription<M>::try_recv_raw_with_attachment(buf, cap,
        out_len, att, att_cap, out_att_len)` over a new FFI
        `nros_cpp_subscription_try_recv_raw_with_attachment` (mirrors
        `try_recv_raw`, using the handle's `try_recv_raw_with_attachment`).
        Verified: native C++ listener builds + links with the header.
  - [ ] **M3.5 — generator emits real service/action callbacks.** Close the M2
        "services/actions still emit C-fn-ptr noops" note by emitting real
        wiring once component callback bodies land — **depends on Phase 172 W.5**
        (component callback bodies, deferred). Track there, not here.

## Acceptance

The only entity-construction surface is: convenient `create_publisher` /
`create_subscription` (+ generic), and `node.publisher(t)` / `subscription(t)`
builders. Zero `verb_noun_axis_axis_axis` identifiers remain. The generator
emits builder calls; the Phase 172 bridge relay is expressible as
`node.subscription(t).generic(..).qos(..).message_info().session(s).build(cb)`.

## Notes

- Scope is the **runtime client API** (nros-node + the rclc/rclcpp/rclrs
  mirrors + the generator's emitted calls), not orchestration — hence its own
  phase, not a 172 sub-item.
- The Phase 172 `[[bridge]]` topic-forwarding runtime half (generator
  `register_bridges` + the relay) lands on top of M1; until then the `nros check`
  `[[bridge]]` warning is its guard.
- **No deprecation window** (2026-05-28): the `register_subscription_*` zoo is an
  internal, unpublished surface — M2 deletes it and migrates callers in one
  change rather than shipping `#[deprecated]` shims. The original M4 ("sweep +
  remove shims") is therefore folded into M2.c–M2.e.
