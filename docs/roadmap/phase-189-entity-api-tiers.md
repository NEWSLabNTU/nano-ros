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
- [~] **M2 — Retire the `register_*_*_*` zoo.** *Generator switched* (codegen
      `d300164`): the subscriber emission now uses
      `executor.node_mut(n).subscription(t).generic(..).qos(..).rx_buffer::<1024>().build(|_d| {})`,
      removing the longest identifier (`register_subscription_raw_with_qos_sized_on`)
      + the dead `noop_raw_subscription` from generated code. Verified end-to-end
      by `nros-cli-core` e2e (generated bare-metal/stm32f4/nuttx/freertos/
      threadx-riscv64/esp32/native fixtures compile+link) + nros-node
      `builder_tests::generator_emitted_chain_compiles`. *Remaining:* `#[deprecated]`
      shims over the builder for the 24-variant executor `register_subscription_*`
      zoo (then delete), and migrate the ~30 example/test callsites (M4 sweep).
      Services/actions still emit C-fn-ptr noops — their builders are M3.
- [ ] **M3 — C / C++ named-options parity (rclc / rclcpp mirrors).** A
      `SubscriptionOptions` / `PublisherOptions` struct on the C/C++ surfaces
      (named fields + defaults, the idiomatic shape there) lowering to the same
      core entity primitive; cbindgen alignment.
- [ ] **M4 — Sweep + remove shims.** Migrate examples / tests / docs to the
      convenient surface + builder; delete the deprecated shims; `grep` shows no
      `register_subscription_*_*_*` outside history.

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
