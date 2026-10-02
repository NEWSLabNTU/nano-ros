---
id: 1602
title: "On the probe road the sizing descriptor's subscription rows name the CALLBACK in `topic`, not the topic"
status: resolved
type: bug
area: [tooling, build]
severity: low
found: 2026-10-01
resolved: 2026-10-03
related: [1265, 1340, 1522, 0827, phase-454, phase-457, rfc-0100]
---

## What

A single-package cargo leaf whose metadata probe runs writes
`build/nros/sizing/<image>.toml` rows from the probe's entity rows, and the
`topic` field of every SUBSCRIPTION row holds the probe row's `id` — the
callback name. Measured 2026-10-01 after `nros sync`:

| leaf | row | `topic =` |
| --- | --- | --- |
| `examples/esp32-c3-baremetal/rust/listener` | subscription `std_msgs/msg/String` | `"on_chatter"` |
| `examples/mps2-an385-baremetal/rust/listener` | subscription `std_msgs/msg/String` | `"on_message"` |
| `examples/esp32-c3-baremetal/rust/talker` | publisher `std_msgs/msg/String` | `"/chatter"` (a publisher's id IS its topic) |

The same esp32 listener, while it still DECLARED its entities in `system.toml`,
wrote `topic = "/chatter"` — so the field changes meaning with the road.

## Why

`leaf_entity_env::declaration_from_probe` builds each row with
`EntityDecl::bare(kind, type, ent.id)` and carries the written topic separately
as `source_topic` (phase-454 W12, precisely because `id` is the callback name
for a subscription). `sizing_descriptor.rs` then reads `d.name` for the row's
`topic` (`endpoint_row(kind, ty, topic, ...)` and `tl_rows`), i.e. the id.

## Impact

Nothing measured mis-sizes from it today: the pool counts are per kind, and the
transient-local rule's publisher rows are keyed correctly because a publisher's
id is its topic. But `topic` is a schema field (`nros-sizing-descriptor`'s
`Endpoint::topic`) exported to cmake as `NROS_SIZING_ENDPOINT_TOPIC`, and a
consumer that joins on it — or a person reading the descriptor — gets a name
that exists nowhere on the wire.

## Fix direction

Prefer `source_topic` over `name` for the row's `topic` where the probe stated
one (one helper, used by both `endpoint_row` and `tl_rows`), and add a probe-road
case to the descriptor tests asserting the topic of a subscription row.

## Resolution

One accessor, `EntityDecl::topic()` — `source_topic` where the probe stated
one, else `name` — and every view that means "the topic" now reads it instead
of `name`. The class was wider than the two descriptor sites named above: the
same `name`-as-topic read sat in `declared_depths`, `declared_qos`,
`queue_depth_defaults`, `buffer_diagnostics`, and the KEEP_ALL / untyped-entity
refusal prose. Sweep: `git grep -nE '\b(e|d|row|r)\.name\b' --
'packages/cli/nros-cli-core/src/*.rs'`. The JSON inventory dump keeps `name`
deliberately: there it is the entity's identity, not a topic.

**What the wrong key cost — measured, and less than the field suggests.**
`contract_join` REWRITES `name` to the contract's resolved spelling on every row
it attributes, so every row carrying stated QoS — the only rows the depth/QoS
tables publish, and the only rows `nros_node::declared_qos::{check,honour}` look
up by `(type, topic)` — already had the right topic:
`examples/native/rust/listener`, which has a contract, wrote `topic = "/chatter"`
before this change. The callback name reached only UNATTRIBUTED probe rows (no
contract, or a refused join), and there it cost:

* every build-script diagnostic that names a subscription (`nros-rmw-zenoh`'s
  ring-depth warnings, `nros-rmw-xrce-cffi`'s ring notes) named the callback;
* `NrosRmwUorbSizing.cmake` dedups its registry by `NROS_SIZING_ENDPOINT_TOPIC`,
  so a publisher and a subscription on one topic counted twice — an over-count,
  the safe direction, on a road (uORB/PX4) the probe does not reach today.

No pool was mis-sized.

**Measured after:** `nros sync examples/mps2-an385-baremetal/rust/listener`
writes `topic = "/chatter"` (was `"on_message"`).

**Test:** `sizing_descriptor::tests::a_probe_road_subscription_row_names_its_topic_not_its_callback`
goes through the real probe reader (`declaration_from_probe`); with the writer
reading `name` again it fails with
`[(Publisher, "/echo"), (Subscription, "on_chatter")]`.
