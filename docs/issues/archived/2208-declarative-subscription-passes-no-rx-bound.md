---
id: 2208
title: "A declarative Rust subscription hands the backend no rx bound, so a declared ingress cannot pace it on XRCE"
status: resolved
type: limitation
severity: medium
area: [rmw, xrce, node, api]
related: [phase-519, rfc-0074, rfc-0038]
found: 2026-11-01
resolved: 2026-11-01
---

## Symptom

Phase-519 W3b carries a contract's `sub.<endpoint>.ingress.rate_hz` to the
backend in `rmw_subscription_options_t::ingress_rate_mhz`. The XRCE backend
turns it into the agent's `max_bytes_per_second` as `rate_hz x
rx_buffer_hint`, and asks for every sample when the hint is 0.

Measured on a copy of `examples/linux/rust/listener` (the declarative
`Component` shape, `rmw = "xrce"`), subscribing `builtin_interfaces/msg/Time`
(wire bound 16 bytes) with `ingress: { rate_hz: 10 }` declared: the rate
reaches the backend, and the backend logs

    subscription '/chatter' declared ingress rate_hz=10.000 but the agent is
    asked for every sample: the type has no static bound, so the declared
    message rate cannot become the agent's byte rate (type bound 0 bytes).

and the image receives the publisher's full 100 Hz. The type HAS a bound. The
same image written against the typed executor API
(`Node::create_subscription::<Time, _>`) is paced to 10.34 Hz.

## Cause

The declarative runtime registers every subscription through the type-erased
generic path (`packages/api/nros/src/node_runtime.rs`,
`create_generic_subscription_with_qos` /
`create_generic_subscription_with_integrity`), which reaches
`Executor::register_subscription_buffered_raw_on` with `backend_hint: None`.
The type `M` is known at declaration time
(`create_subscription_for_callback_*` calls `register_declared_type::<M>()`),
but its `MAX_SERIALIZED_BOUND` is not recorded in the entity metadata, so
nothing downstream can state it.

The same gap affects zenoh-pico's receive size class (RFC-0038), which reads
the same hint: every declarative Rust subscription takes the backend's default
class.

## Fix direction

Record the type's bound (`nros_node::rmw_type_registry::subscription_rx_hint`)
in the declared entity's metadata at declaration time, and pass it as the
backend hint on the generic registration. The in-place and buffered rows of
issue 1319 are unaffected: the hint is the BACKEND's number, not the arena
slot's.

## Resolution (2026-11-01)

The declaration records the bound and the runtime hands it on.

- `EntityMetadata::rx_bound` is set from `RosMessage::MAX_SERIALIZED_BOUND` by
  every subscription declaration in `packages/api/nros/src/node.rs`: the plain
  and QoS forms, the `.safety()` form and `create_subscription_static`. It
  stays `None` for an unbounded type and for every other entity kind.
- `nros-node` gains `create_generic_subscription_with_qos_and_rx_hint` and
  `create_generic_subscription_with_integrity_and_rx_hint`. The old names pass
  `None`. `register_subscription_buffered_raw_on` and its safety twin take the
  hint and put it in `SubscriptionRequest::backend_hint`.
- The declarative runtime (`node_runtime.rs`) passes `metadata.rx_bound` on
  both arms.

Test: `a_declared_subscription_records_its_message_bound` in
`packages/rmw/metadata/tests/component_registration.rs`, with an unbounded type
and a publisher as negative controls.

Measured with the issue's shape (a `Component` subscribing
`builtin_interfaces/msg/Time`, `rmw = "xrce"`, depth 1, a Humble publisher at
100 Hz, 15 s): with `ingress: { rate_hz: 10 }` it received 10.34 Hz at a median
age of 100.5 ms; with no `ingress`, 100.07 Hz at 0.4 ms.

The zenoh receive class reads the same hint, so declarative subscriptions on
zenoh now take the class their type's bound selects rather than the default.
