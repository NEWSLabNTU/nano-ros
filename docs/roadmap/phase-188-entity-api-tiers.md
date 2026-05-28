# Phase 188 — Entity API tiers (convenient + customizable builder)

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

- [ ] **M1 — Rust entity builder + convenient surface (unblocks 172 bridge).**
      `node.publisher(topic)` / `node.subscription(topic)` builders with knobs
      `.typed::<M>()` / `.generic(ty, hash)`, `.qos()`, `.rx_buffer::<N>()`,
      `.session(slot)`, `.message_info()`, `.sched_context()`, `.build()`.
      Convenient `create_publisher` / `create_subscription` (+ `create_generic_*`)
      re-pointed at the builder (thin wrappers, defaults). rclrs-style fluent
      QoS-on-topic kept. The `.message_info()` + `.session()` knobs are what the
      Phase 172 bridge relay needs.
- [ ] **M2 — Retire the `register_*_*_*` zoo.** One release as `#[deprecated]`
      shims over the builder, then deleted. **The generator emits builder calls**
      (replacing the `register_subscription_raw_with_qos_sized_on` etc. it emits
      today). No long identifier survives.
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
