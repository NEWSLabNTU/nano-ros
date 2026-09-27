---
id: 1522
title: "A subscription's registration path is OBSERVED at `open_subscription`, so
  it is unstated on every road whose probe declares without registering — the
  Rust producer, the `ENTITIES` grammar, a launch declaration and every service
  endpoint"
status: open
type: tech-debt
area: [build, core]
severity: medium
found: 2026-09-28
related: [1340, 1319, 1393, 1407, 0196]
---

## What is open

phase-457 W3 made `[[endpoint]] registration_path` answer per ENDPOINT instead of
per image. The half that is per-endpoint is the CALL SITE's own answer to *can
this delivery shape dispatch out of the backend's own receive slot* —
`SubscriptionRequest::in_place_capable`, stated at each of the executor's eleven
subscription entry points and read at the one site that asks
`supports_process_in_place`.

Nothing outside the executor can derive it. So it is **observed**:
`nros_node::executor::registration_observer` reports it from
`Executor::open_subscription`, the metadata probe records it on the subscription
row it belongs to, and the sidecar carries it as `in_place` (schema v3). Where
nothing observed a row, `registration_path` REFUSES and every consumer budgets
the receive region — the direction that cannot ship
`NodeError::BufferTooSmall`.

**Four populations are therefore unstated**, and each is unstated for its own
reason:

| population | why nothing observed it |
| --- | --- |
| a **Rust** component's endpoints | `record_node_metadata::<C>` runs the component's `register()` against a recording `NodeContext` and opens no executor, so no registration happens in the probe at all |
| a row from the **`ENTITIES` grammar** (`system.toml` `[[component]] entities`) | a declaration names the entity; which of eleven overloads its code calls is a property of the source |
| a row from a **launch declaration** (`EntityInventory::from_model`) | the same, one road over |
| every **service / action** endpoint | `open_subscription` is the subscription prologue; a service server's request buffer is the same question and has no equivalent observation site |

## What it costs, measured

Nothing today, and that is the point of filing it rather than fixing it here.
Measured on `examples/native/rust/listener` (one `KEEP_LAST(1)` subscription,
zenoh), `arena_model::REQUIRED`:

| descriptor row | `REQUIRED` |
| --- | --- |
| observed in-place-capable | 3,072 |
| observed NOT capable | 6,144 |
| **unobserved (this issue)** | **6,144** |

So an unobserved row is priced exactly as a buffering one — the safe direction,
and the number every image was built against before W3. What it forgoes is issue
1340's saving, measured at **10,840 bytes per subscription** on a
four-subscription image of the same leaf (`EXECUTOR_BACKING` 61,184 → 17,824 B,
`ARENA_SIZE` 51,552 → 8,192 B).

## What closing it looks like

Two independent pieces, and the first is much smaller than it looks:

1. **The Rust probe.** Its registrar is ONE function —
   `nros::node_runtime`'s `EntityKind::Subscription` arm, which lowers every
   declared subscription to `create_generic_subscription_with_qos` (or
   `_with_integrity` for a `.safety()` one). So the shape a declared endpoint
   will take is a function of the DECLARATION, computed in one place. Either make
   the probe register for real, or have that one registrar and the recorder read
   ONE classifier — the second is cheaper and is not a second opinion **only if
   the classifier is the thing the registrar branches on**, gated. Writing a
   parallel "what would the declarative road do" table in the CLI is issue 0196's
   class and is what W3 removed.

   Worth knowing before starting: today both declarative arms answer `false`
   (the generic path is not in-place capable), so closing this makes the Rust
   road state `unbounded` rather than unlock the saving. The saving arrives only
   with issue 1340's own first candidate — letting
   `register_subscription_buffered_raw_on` take the in-place row, which its
   `in_place_capable: false` writes down as deliberate and unfinished.

2. **A service endpoint's registration.** `open_subscription` has no sibling on
   the service path. When phase-454 W6.b's pricing starts reading a service
   row's `registration_path`, this is what it needs; until then the composed
   answer stands and nothing reads it.

## Do not

Do not default an absent observation to `false`, and do not infer it from the
entry's LANGUAGE. The second is what `registration_path` did until W3 and it was
wrong for nine of the executor's eleven entry points — free while the in-place
row was priced at the type's bound, and an UNDER-size the moment the row is
priced at what it actually claims.
