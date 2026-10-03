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

## Fix direction, revised 2026-10-03 (RFC-0100 Amendment 1)

*(Placed above the progress log so it is read first; the log below is what
landed.)*

* **The field `RX_BUF` needs is now decided: `[types] max_wire_bound_bytes`** —
  the largest wire bound over every type the image's interface closure
  REGISTERED, taken per column like `[types]`' other three maxima. It refuses
  naming the type when a registered type is unbounded or unpriced, and naming
  the table when a registered table is absent (a clean tree's first configure —
  issue 1647). Same inputs as `[types]` today (phase-457-payload W2's
  registered bound tables), so every producer road can state it; in an N:1
  cmake configure the registration is already configure-wide, i.e. it IS the
  shared runtime's closure, and needs no reduction (D12 rule 3). It is a
  `schema_version` bump, which D12's `[meta]` change shares — land the two in
  one bump, not two.
* **Then the reader:** `nros-node/build.rs` ranks it first for
  `NROS_SUBSCRIPTION_BUFFER_SIZE`, the carrier second — the shape the three
  subscriber classes already have.
* **The road for all four is issue 1649 (RFC-0100 D12),** not this issue. After
  the field and the reader land here, the four payload rows retire together with
  1649's, by the W14 knob diff.

Files: `packages/tooling/nros-sizing-descriptor` (schema + reader),
`packages/cli/nros-cli-core/src/sizing_descriptor.rs` (the composer),
`packages/core/nros-node/build.rs` (the `RX_BUF` reader). **Overlap:** the
composer and schema are also where 1649's D12 lands — one owner for both, so
the schema moves once.

## Progress, 2026-10-03 — the three subscriber classes have a reader; `RX_BUF` is what is left

**Wired (RFC-0100 D5).** `nros_sizing_descriptor::subscriber_payload_classes`
computes the small/large classes from the descriptor's `subscription` rows, with
the CALLER's ceiling, and `nros-rmw-zenoh/build.rs` ranks it FIRST for
`NROS_SUBSCRIBER_BUFFER_SIZE`, `ZPICO_SUBSCRIBER_LARGE_SIZE` and
`ZPICO_MAX_LARGE_SUBSCRIBERS`; the `NROS_DECLARED_*` carrier is the next rung.
The classification is `PayloadClasses::add` — ONE Rust spelling, which the
cargo-leaf join (`leaf_payload_classes::join`) now calls too (issue 1025). The
CMake twin stays `_nros_bounds_publish_payload_classes`.

It refuses (and the carrier rung answers) when the rows cannot be the whole
subscribed set — `[image] subscription_entities` unstated or unequal to the row
count — or a subscription row has no `wire_bound_bytes`. A class derived over a
subset is the under-size D6 forbids.

**Measured**, `nros-rmw-zenoh`'s generated consts, the descriptor being
`examples/workspaces/cpp`'s `native_entry` (one `std_msgs/msg/Int32`
subscription, bound 12) and the carriers its own configure emitted:

| inputs | `SUBSCRIBER_BUFFER_SIZE` | `MAX_LARGE_SUBSCRIBERS` | `SUBSCRIBER_LARGE_SIZE` |
| --- | --- | --- | --- |
| neither (control) | 1024 | 2 | 16384 |
| carriers only (`=12`, `=0`) | 12 | 0 | 16384 |
| descriptor only | 12 | 0 | 16384 |
| descriptor + conflicting carriers (`=999`, `=3`) | 12 | 0 | 16384 |
| a REFUSING descriptor (`examples/native/rust/listener`, unbounded `String`) + those carriers | 999 | 3 | 16384 |

No in-tree image changes size: the one bounded-subscription image with a
contract (`examples/workspaces/cpp`) is a MULTI-ENTRY configure, which names no
descriptor to cargo, so its carriers still decide — and they agree.

**Ledger.** The three rows moved from this issue to issue 1407: the reason that
keeps them is now the ROAD (a multi-entry configure and a Zephyr west entry name
no descriptor to cargo), not a missing reader. (Issue 1407 then closed the Zephyr
west road, and the rows moved on to issue 1649, the multi-entry configure.)

**Still open here:** `NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE` (`RX_BUF`). Its
basis is the CLOSURE — every type the image could receive or publish, because
`DEFAULT_TX_BUF` aliases it — and no set of `[[endpoint]]` rows spans that, so it
needs a descriptor field (an RFC-0100 D4 amendment) before any reader can exist.

## Progress, 2026-10-03 — the field exists (schema 2); the reader is what is left

`[types] max_wire_bound_bytes` landed in the schema-2 bump it shares with issue
1649 (RFC-0100 D12). Every producer road fills it through ONE rule,
`leaf_take_buffer::derive` — the cargo leaf's `NROS_SUBSCRIPTION_BUFFER_SIZE`
derivation, called rather than restated (issue 1025) — over every type of every
bound table the image REGISTERED, never over the endpoint rows. It refuses
naming the open types, or naming the absent table (`bounds_error`), and is
projected to cmake as `NROS_SIZING_TYPES_MAX_WIRE_BOUND_BYTES`.

Measured on `examples/workspaces/cpp`'s native runtime descriptor: REFUSED,
naming 38 unbounded types (`example_interfaces/msg/String`, the `*MultiArray`
family, ...) — the same answer the cmake take-buffer derivation gives there
(`NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE` is not emitted for that configure).

**Still open:** the `nros-node` reader, and the retirement question for the
carrier.
