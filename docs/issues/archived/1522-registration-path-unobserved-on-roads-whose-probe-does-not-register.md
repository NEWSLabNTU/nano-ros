---
id: 1522
title: "A subscription's registration path is OBSERVED at `open_subscription`, so
  it is unstated on every road whose probe declares without registering — the
  Rust producer, the `ENTITIES` grammar, a launch declaration and every service
  endpoint"
status: resolved
type: tech-debt
area: [build, core]
severity: medium
found: 2026-09-28
resolved: 2026-09-28
resolved_in: "phase-457 W5 — piece 1 (the Rust probe's SUBSCRIPTION rows). The `ENTITIES` grammar, a launch declaration and every service endpoint still refuse, on purpose: piece 2 needs phase-454 W6.b's pricing to read a service row first, and file it fresh when it does"
related: [1340, 1319, 1393, 1407, 0518, 0196]
---

> **RESOLVED 2026-09-28 (phase-457 W5), for the first of the two pieces.**
> The Rust probe now STATES its subscriptions' `registration_path`; the other
> three populations still refuse, deliberately and for their own reasons. See
> "What was done" at the bottom, which also records the defect this work
> UNCOVERED — W3's `in_place` reached one CLI reader and not the
> `deny_unknown_fields` one every producer's sidecar is parsed through, so
> emitting the key from a second road broke `nros build` outright.

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

## What was done

### The route, and why it is not a second opinion

The second of the two this issue offered: **one classifier, read by the
registrar and by the recorder**. `nros_node::executor::declared_shape::
DeclaredSubscriptionShape` has one variant per lowering the declarative
registrar can choose (`BufferedRaw`, `BufferedRawSafety`).

The condition this issue attached to that route — *not a second opinion ONLY
IF the classifier is the thing the registrar branches on* — is met twice over,
and the second half was not in the issue's own framing:

1. **The registrar branches on it.** `node_runtime`'s
   `EntityKind::Subscription` arm no longer tests `metadata.safety`; it asks
   for a shape and dispatches on the answer. The `safety-e2e` mask — with the
   capability off the runtime IGNORES the flag — has exactly one home,
   `EntityMetadata::declared_subscription_shape`, which is the only caller of
   `of_declaration`. Two masks would be the same disagreement one level down.
2. **The `bool` has one definition.** Each entry point's
   `SubscriptionRequest::in_place_capable` is now
   `DeclaredSubscriptionShape::<V>.in_place_capable()` rather than a literal.
   Without this the classifier would still be a COPY of the entry point's
   answer, which is the thing this issue warns against with a different shape;
   with it, issue 1340's candidate is a one-line change that moves the
   executor and the probe together.

The recorder side is `MetadataRecorder`'s `NodeRuntime::create_entity` — the
ONE seam a Rust declaration crosses. The C/C++ adapters reach the recorder
through `push_entity`, so their OBSERVED fact still wins its own way and this
change cannot overwrite it.

Gate: `check-declared-subscription-shape`, on the fast line. Every expectation
is harvested from the enum's own variants and their doc comments (which name
the entry point and the lowering) — an authored table of entry points in the
gate would be this issue's own 0196 warning one level up. Five mutations plus
a vacuity control run on the normal path, each with its own diagnosis.

### What it bought, measured

**Correctness, not bytes — as this issue predicted.** Both declarative arms
answer `false`, so a Rust subscription moves from REFUSED to a stated
`unbounded` row. That is what the refusal already priced, and the descriptor's
two byte-moving predicates agree on the pair: `claims_no_receive_region` is
`false` for both (only a STATED `in_place` removes a region) and
`may_claim_closure_buffer` is `true` for both (`Refused` and
`Stated(Unbounded)` are the same answer there).

Measured on `examples/native/rust/listener` — the only in-tree leaf with a
`system.contract.yaml` on a cargo road, hence the only image whose descriptor
this can move — built both ways, `just mem-report`:

| | before | after |
| --- | --- | --- |
| RAM (`.bss` + `.data`), by section | 181,434 | 181,434 |
| RAM attributed to symbols | 157,554 | 157,554 |

**No image changed size, in either direction.** The descriptor row itself went
from a refusal naming this issue to `registration_path = "unbounded"`.

Two populations cannot change by construction and were not built: an image on
a backend that does not dispatch in place never reaches the
`observed_in_place_capable` test at all, and an image with no contract has no
descriptor.

### What stays open

* **The `ENTITIES` grammar** and **a launch declaration**. Both are CLI-side
  (`EntityInventory::from_model` / `system.toml`'s `[[component]] entities`)
  and neither has a registrar to be consistent WITH — a classifier there would
  be the parallel table this issue forbids, so they keep refusing.
* **Every service / action endpoint** — this issue's second piece. Unchanged:
  `open_subscription` still has no service sibling, and nothing reads a
  service row's `registration_path` yet (phase-454 W6.b's pricing is what
  would). Left as the issue asked.

### What this work UNCOVERED

`in_place` had never been parseable by
`nros-cli-core/src/orchestration/source_metadata.rs`'s `SourceSubscriber`,
which is `deny_unknown_fields`. `metadata_refresh::stamp_provenance` runs
EVERY producer's sidecar through those structs — its own comment says the
round-trip "doubles as schema validation of what the harness emitted" — so the
key does not get ignored, it fails the whole document with `unknown field
'in_place'`, four frames from the writer.

Nothing caught it for a fortnight because no producer that reaches that reader
emitted the key: the Rust probe observed nothing, which is this issue. Making
the Rust road state the row is what first sent an `in_place` through, as a
failed `nros build` of `examples/native/rust/listener`.

Second occurrence of the class (issue 0518's `period_us` is the first, and the
struct's own doc comment records it), so it is gated rather than fixed again:
`check-sidecar-endpoint-keys` pairs `write_<x>_json` to `Source<CamelX>` by
name and requires every directly-written key to be declared. Its reach — keys
the paired writer emits itself, not through a shared helper — is stated in the
script rather than implied.

**And the two roads fail DIFFERENTLY, which is why the C/C++ one was never
reported.** The Rust branch propagates (`stamp_provenance(&sidecar, &digest)?`)
and stops the sync with the message above. The C/C++ batch branch is
`let _ = stamp_provenance(...)` — deliberately, so one unstampable component
does not fail a whole batch — so there the refusal is SILENT: the sidecar never
gets its provenance, `sidecar_is_fresh` is false for ever, and the symptom is a
C/C++ probe project that reconfigures and rebuilds on EVERY `nros sync` with
nothing said. A slow, invisible cost rather than an error, which is the kind
nobody files.
