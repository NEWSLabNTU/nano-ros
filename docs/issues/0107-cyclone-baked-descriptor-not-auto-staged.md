---
id: 107
title: "Cyclone egress in a schema-free data-driven bridge (`run_from_config`) fails `PublisherCreationFailed` — no descriptor staged, and user payload types are NOT baked"
status: open
type: bug
area: rmw
related: [phase-267, rfc-0009]
---

## Summary (corrected 2026-06-27 after study)

The Cyclone egress publisher in `nros_bridge::run_from_config` fails
`PublisherCreationFailed`: `publisher_create` (`publisher.cpp:123`)
→ `find_descriptor("std_msgs::msg::dds_::Int32_")` returns null because **no
descriptor for the forwarded type is staged**.

**My earlier premise was WRONG.** `std_msgs/Int32` is NOT a baked default.
`nros-rmw-cyclonedds-sys/build.rs` (lines 70-109) bakes **only**
`rmw_dds_common_graph` (the RMW-intrinsic discovery descriptor). Its comment is
explicit: *"Every user payload type (`std_msgs/Int32`, …) goes through the
build-dep helper `nros_build::cyclonedds::Descriptors` from the consumer's
`build.rs` (Phase 212.K.4). Never hard-code a user message type here."* — and that
helper **does not exist yet**. So Cyclone requires a per-type descriptor that is
staged by the CONSUMER, and the only staging paths today are:
- a consumer `build.rs` (the unbuilt `nros_build::cyclonedds` helper), or
- a runtime `nros_rmw::register_type_descriptor(name, &[Field])` call (every
  working Cyclone consumer, incl. `bins/bridge-zenoh-to-cyclonedds-fwd`, does this
  with a hand-written `&[Field]`).

`run_from_config` is schema-free (only the type NAME from `nros-bridge.toml`), so
it does neither → the egress pub fails.

Secondary finding (latent, not the blocker): even the baked
`rmw_dds_common_graph` descriptor's `__attribute__((constructor))` does NOT run
in a consumer — the descriptor lib is DCE'd unless a symbol is referenced, and
there is no `#[used]` anchor for it (only the platform lib has one,
`nros-rmw-cyclonedds-sys/src/lib.rs`). It works in practice only because
`graph.cpp` *explicitly calls* `register_rmw_dds_common_graph_0()` at runtime. No
key mismatch: `register_descriptor` keys under both the passed name AND
`descriptor->m_typename`, both DDS-mangled — if a ctor ran, lookup would match.

## What makes this tractable

`nros_rmw::register_type_descriptor(name, &[Field])` + the runtime
`dynamic_type_builder` (`nros-rmw-cyclonedds/bridge/dynamic_type_builder.cpp`)
synthesise a full `dds_topic_descriptor_t` from a **runtime `&[Field]` array** —
no `idlc`, no build.rs. The generated message crate already carries that schema as
`<M as Message>::FIELDS`. So a Cyclone descriptor CAN be staged at runtime with no
build.rs — the open question is how the schema-free bridge entry obtains the
schema.

## Fix directions (no user-written build.rs — see phase-267)

- **(A) Typed staging emitted by the macro.** `nros::main!` (which already reads
  `nros-bridge.toml`) maps each cyclone-side forwarded type → its Rust path
  (`std_msgs/msg/Int32` → `std_msgs::msg::Int32`) and emits
  `nros_rmw_cyclonedds::register_type::<std_msgs::msg::Int32>()` before
  `run_from_config_str`. Reuses the canonical compile-time `M::FIELDS` (issue #67's
  typed descriptor hook, gated `rmw_cyclonedds_present`). The Entry deps the msg
  crates (it forwards them) + `nros-rmw-cyclonedds`. No build.rs, no schema in
  config, no offset math in sync.
- **(B) Schema in config + runtime stage.** `nros sync` derives the `&[Field]`
  schema (name, `FieldType`, offset) from the `.msg` and emits it into
  `nros-bridge.toml`; `run_from_config` stages it via `register_type_descriptor`.
  Fully data-driven, but sync must compute field offsets (size/alignment) — the
  hard part.
- **(C) Generated descriptor crate.** `nros sync` runs `nros codegen
  cyclonedds-descriptors` (`.msg` → IDL → `idlc` → `register.c`) into a GENERATED
  crate (with its own build.rs, not the user's) that the Entry deps; the
  generated ctor stages it. General (any type), heaviest.

(A) is the cleanest for the Rust bridge (reuses `M::FIELDS`, no offset math); (B)
keeps the config fully self-describing; (C) is the most general. Also fixed in
phase-267: the type NAME is DDS-mangled in `nros-bridge.toml`
(`render_bridge_runtime_config` via `interface_type_name`).
