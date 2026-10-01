---
id: 1602
title: "On the probe road the sizing descriptor's subscription rows name the CALLBACK in `topic`, not the topic"
status: open
type: bug
area: [tooling, build]
severity: low
found: 2026-10-01
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
