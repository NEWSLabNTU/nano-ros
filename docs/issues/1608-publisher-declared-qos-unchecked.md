---
id: 1608
title: "A PUBLISHER's declared QoS is delivered to the build and compared with nothing — the declared-QoS check covers subscriptions only"
status: open
type: enhancement
area: [build, core]
severity: low
found: 2026-10-01
related: [1256, 1564, rfc-0100, phase-454]
---

## What

A contract can state `qos:` for a publisher endpoint (`pub_endpoints.<ep>.qos`)
and every road carries it: phase-454 W2 made a publisher's depth travel, W3
the three policies, and the sizing descriptor states all four per
`[[endpoint]]` row. `transient_local` on a publisher is the one QoS fact that
is publisher-side by nature -- it is what a zenoh image spends a queryable slot
on (`nros_sizing_descriptor::transient_local_publishers_over`).

The code/contract AGREEMENT check covers SUBSCRIPTIONS only, in all three
languages, after issue 1256 widened it from the depth to reliability and
durability:

| surface | what it checks | publishers |
| --- | --- | --- |
| `nros_declared_qos_generated.h` (C/C++ compile time) | rows keyed `(type, topic)` | none -- a publisher and a subscription on one pair would be two rows with one key, so the table has no publisher row (`EntityInventory::declared_qos_header_table`) |
| `Node::check_declared_qos` (C++ boot) | `create_subscription_in*` | no publisher counterpart |
| `nros_node::declared_qos::{check,honour}` (Rust / FFI registration) | the subscription registration seams | `build.rs` reads `EndpointKind::Subscription` rows only |

So a publisher whose code passes `volatile` against a contract that declares
`transient_local` (late joiners get nothing, and the image was sized for a
queryable it never declares) -- or the reverse (a queryable slot the build did
not count) -- builds and boots silently.

## Why it was not done with 1256

1256's open bullet was that the EXISTING header and boot check cover
reliability and durability; both were subscription-only, for depth as well.
Adding publishers is a new surface, not a wider column: the table needs a KIND
column (and `NROS_ASSERT_DECLARED_*` a publisher spelling), C++ has no
`NROS_PUBLISH` macro seam to assert at -- `create_publisher_in` is called
directly -- and the Rust publisher registration paths have no `honour` funnel
yet.

## Acceptance

A publisher whose code and contract disagree on reliability or durability
fails the build (C/C++, where the QoS is a constant expression) or the
registration (Rust, and every non-constant C/C++ call site), with a negative
control in `just check declared-qos-header` and `just check
declared-qos-registration` the way the subscription side has one.
