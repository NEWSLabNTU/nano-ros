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
  - [ ] **M3.3 — Services / actions QoS + options parity.** *Blocked on core
        plumbing* (found during M3.1/M3.2): the Rust core service create
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
        [`docs/design/service-qos-gap.md`](../design/service-qos-gap.md).
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
        `tests::test_raw_subscription_info_callback`. **C++ deferred:** C++
        subscriptions are poll-style (no callback) — surfacing the attachment
        there wants a `Subscription<M>::take_with_info(...)` poll accessor (fits
        the poll model better than a callback), tracked as M3.4b.
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
