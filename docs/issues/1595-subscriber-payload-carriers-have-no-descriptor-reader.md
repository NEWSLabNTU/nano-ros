---
id: 1595
title: "No consumer reads the subscriber payload classes off the sizing
  descriptor — zenoh's small/large classes and nros-node's RX_BUF are sized from
  `NROS_DECLARED_*` (cmake) or plain knob rows (cargo leaf), so the four payload
  carriers cannot retire even though the descriptor now states the bounds on every
  producer road"
status: open
type: tech-debt
area: [build, core]
severity: low
found: 2026-10-01
related: [1393, 1407, 1199, 1233, 1122]
---

## What is open

`check-knob-single-reader`'s ledger KEPT four payload-class carriers against
issue 1393, whose remedy was to let the model-road descriptor STATE
`[[endpoint]] wire_bound_bytes`. That remedy has landed (phase-457-payload W2
hands every producer road its registered bound tables, and issue 1393's closure
composes the rest with the leaf road's own code). The ledger's rule — "if the
blocker closed, re-run the per-fact test" — was re-run for all four, and they
still cannot retire, for a reason that was never about the descriptor's fields:

| carrier | consumer | reads the descriptor? |
| --- | --- | --- |
| `NROS_DECLARED_SUBSCRIBER_BUFFER_SIZE` | `nros-rmw-zenoh/build.rs` (small class) | no — `env_usize_rung("NROS_SUBSCRIBER_BUFFER_SIZE", declared_usize(...))` |
| `NROS_DECLARED_LARGE_SUBSCRIBERS` | `nros-rmw-zenoh/build.rs` (large count) | no |
| `NROS_DECLARED_SUBSCRIBER_LARGE_SIZE` | `nros-rmw-zenoh/build.rs` (large size) | no |
| `NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE` | `nros-node/build.rs` (`RX_BUF`) | no — `env_usize_declared(...)` |

And the cargo-LEAF road does not use the descriptor for these either: it writes
the same facts as plain knob rows (`leaf_entity_env::DERIVED_PAYLOAD_ENV_KEYS`,
`DERIVED_CLOSURE_ENV_KEYS`). So the descriptor stating each subscription's bound
buys nothing for these four today — the same shape the ledger already records
for `NROS_DECLARED_SERVICE_SERVERS` ("no consumer reads the count OFF the
descriptor yet").

## Two different amounts of work

* **The three subscriber classes** (`SUBSCRIBER_BUFFER_SIZE`,
  `LARGE_SUBSCRIBERS`, `SUBSCRIBER_LARGE_SIZE`) are a function of the
  SUBSCRIBED rows' bounds, which the descriptor carries per row
  (`basis = "contract"`, `wire_bound_bytes` on each subscription). A consumer
  can derive them descriptor-first, the way `declared_service_request_bytes`
  does for the service inbox, with the carrier ranked below. The derivation
  rule (the small/large ceiling) lives in `NanoRosMessageBounds.cmake` and the
  leaf road; a descriptor-side reader must SHARE it, not restate it (issue
  1025).
* **`SUBSCRIPTION_BUFFER_SIZE`** is a CLOSURE-basis fact: one global `RX_BUF`
  that must hold every type the image could receive OR publish, because
  `DEFAULT_TX_BUF` aliases it. No set of `[[endpoint]]` rows spans the closure
  (an undeclared endpoint's type is in the closure and in no row), so this one
  needs a descriptor FIELD, not just a reader — an RFC-0100 D4 amendment.

## Then the roads

Even with readers, a Zephyr west entry and a multi-entry cmake configure name no
descriptor to cargo (issue 1407), so the carriers stay for those roads until that
closes. This issue is the one that has to move FIRST: on a road that does name a
descriptor, retiring a carrier nobody reads the replacement for would lose the
fact outright.
