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

**Status.** Not started — design approved
([`docs/design/entity-api-tiers.md`](../design/entity-api-tiers.md)), split out
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
- [~] **M2 — Remove the `register_subscription_*` zoo + migrate all callers.**
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

      **Slices:** *M2.a* builder typed `.message_info()` + `.safety()` knobs.
      *M2.b* settle + `pub(crate)` the 3 closure cores; introduce the clean C-FFI
      core. *M2.c* migrate Rust callers (14 examples, ~45 unit tests, 4 benches/
      integration, 4 doc comments, 5 node.rs delegations). *M2.d* re-point the 2
      C-FFI callsites. *M2.e* delete the public zoo; `grep` shows no
      `register_subscription_*` public method outside the kept cores; `just ci`.
- [ ] **M3 — C / C++ named-options parity (rclc / rclcpp mirrors).** A
      `SubscriptionOptions` / `PublisherOptions` struct on the C/C++ surfaces
      (named fields + defaults, the idiomatic shape there) lowering to the same
      core entity primitive; cbindgen alignment. (Also gives services/actions
      their builders — they still emit C-fn-ptr noops today.)

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
