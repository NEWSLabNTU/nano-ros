---
id: 1319
title: "The arena prices a subscription at the SUBSCRIBED payload class, and a backend without type descriptors allocates the CLOSURE buffer — the model is below the allocation on zenoh and XRCE"
status: resolved
type: bug
area: executor, build
severity: medium
found: 2026-09-11
resolved: 2026-09-12
related: [issue-1255, issue-1190, issue-1340, phase-403, phase-448, phase-454]
---

## Resolution — phase-454 W5.b

The arena derivation no longer prices every subscription at its type's bound.
`nros-node/build.rs` reads the sizing descriptor's per-endpoint
`registration_path` (RFC-0100 D1/D4, landed W4) and charges each row what THAT
path claims: the type's own bound where the registration reaches it, and the
closure buffer `RX_BUF` where it does not. A path the descriptor could not state
is priced at the closure buffer and says so as a `cargo::warning` — the safe
direction, loudly, per D6.

That is this issue's SECOND candidate fix. The first leaves the build blind; the
third is ruled out below as *"the answer that goes stale next time a path
changes"*. It is explicitly **not** *"raise the term back to `RX_BUF`"*, which
would give up issue 1255's saving on the two paths where the per-type bound IS
what is allocated.

Measured — derived arena for one `KEEP_LAST(1)` subscription at the reference
island's numbers (`NROS_SUBSCRIBER_BUFFER_SIZE=880`,
`NROS_SUBSCRIPTION_BUFFER_SIZE=1496`):

| descriptor says | `arena_model::REQUIRED` | slot |
| --- | --- | --- |
| no descriptor | 5,712 | 880 |
| `c_typed_hint` | 5,712 | 880 |
| `rust_typed_descriptors` | 5,712 | 880 |
| `rust_typed_in_place` | 5,712 | 880 |
| `rust_typed_schemaless` | **7,560** | 1,496 |
| `c_raw_no_hint` | **7,560** | 1,496 |
| path REFUSED | 7,560, plus a warning naming the refusal | 1,496 |

`7560 − 5712 = 1848` — this issue's own number, `3 × (1496 − 880)`. With no
descriptor every emitted VALUE is byte-identical to the build before the wave
(diffed against `bc7ae4617`; the only textual difference is the new
`arena_model::PUBSUB_SLOT_BYTES` const, whose value is the number the derivation
already used).

## Reproduced — and the analysis needed one correction

**Reproduced**, in
`executor::tests::a_schemaless_subscription_outgrows_an_arena_priced_at_its_types_bound`.
An arena sized at what the OLD model budgets for one subscription refuses the
registration with `NodeError::BufferTooSmall`; the same registration fits the
number W5.b derives, and the shortfall is the slot count times the difference in
slot size, exactly as predicted here.

**The correction, which the reproduction is what found.** The table below
assigns zenoh and XRCE to the `RX_BUF` row.
`register_subscription_buffered_on` asks `handle.supports_process_in_place()`
**before** it computes a slot size; both schemaless backends answer an
unconditional `true`, and the registration returns through `SubInplaceEntry`
having allocated no receive region at all. Measured on `contract-monitor-sub`
over zenoh: a `std_msgs/Header` subscription claims **672 bytes** of arena
against a 9,768-byte budgeted region.

So the UNDER-size described here is real and is fixed, but on those two backends
it is not reachable through a Rust TYPED registration. It is reachable through
`c_raw_no_hint`, and through `rust_typed_schemaless` on a schemaless backend
that buffers. The measured fifth row is `RegistrationPath::RustTypedInPlace`,
and the OVER-statement it exposes — 9,768 bytes per subscription on every zenoh
and XRCE image — is
[issue 1340](1340-arena-budgets-a-receive-region-an-in-place-backend-never-claims.md),
which also records why W5 did not take that saving (the Rust GENERIC
registration on the same backend does not reach the capability test).

---

## What the model says and what the runtime does

`nros-node/build.rs` prices a subscription's receive region at `rx_recv_size`
(`NROS_SUBSCRIBER_BUFFER_SIZE`, the largest bound among the types the image
SUBSCRIBES to) and, since issue 1255, at each type's own bound where the bound
table reaches it. Both are at or below the CLOSURE buffer
`NROS_SUBSCRIPTION_BUFFER_SIZE` (`DEFAULT_RX_BUF_SIZE`, the const generic
`RX_BUF`). On the reference island that is 880 against 1,496.

What the runtime allocates depends on the registration path, and the four are
not the same:

| path | slot size |
| --- | --- |
| C/C++ typed, `rx_buffer_hint != 0` (`nros::rx_size_bound<M>`) | the type's own `_RX` — matches the model |
| Rust typed, backend WITH type descriptors (Cyclone) | `min(transport_framed(bound), RX_BUF)` — at or below the model |
| Rust typed, backend WITHOUT type descriptors (zenoh, XRCE) | **`RX_BUF`** |
| C/C++ raw with no hint | **`RX_BUF`** |

*(The third row is CORRECTED above: those two backends dispatch in place and
claim no region. The row stands for a schemaless backend that buffers.)*

The last two rows are the gap. `default_subscription_rx_bytes::<M>` has two
arms, and the `#[cfg(not(rmw_needs_type_descriptors))]` one returns `None` for
every type — correctly, because `MessageForRmw` carries no schema there and no
bound is reachable at a type-erased site. `spin.rs`'s
`register_subscription_buffered_on` then takes `.unwrap_or(RX_BUF)`. So on a
zenoh or XRCE image every default-registered Rust subscription claims
`buffered_region(depth, RX_BUF)` while the arena budgeted
`buffered_region(depth, rx_recv_size)`.

On the island's numbers, depth 1, that is `3 x (1496 - 880)` = **1,848 bytes
per subscription** the model does not hold.

## This predates issue 1255 and is not caused by it

The term has read `NROS_SUBSCRIBER_BUFFER_SIZE` since phase-403 step 3, whose
own comment argues the case: "`register_subscription_buffered_*` sizes it from
the type's own bound, and the SUBSCRIBER payload class is the maximum over
subscribed types, so it is a tight upper bound for every subscription". That
sentence is true of the C/C++ typed path and of a descriptor-carrying backend,
and it is false of the other two rows. Issue 1255 narrowed the same term from
the class maximum to the per-type bound; it inherited this gap rather than
introducing it, and the per-type table is `min`-safe against the second row
(`min(framed(bound), RX_BUF) <= bound`), so the gap is exactly the two `RX_BUF`
rows and no wider than it was.

## Why it has not bitten

`arena_model::REQUIRED` is non-zero only where every `NROS_ENTITY_COUNT_*`
arrives, which today is the Zephyr resolver road alone; elsewhere the
derivation keeps the pre-step-3 worst case, which budgets every slot at the
action-client entry and is far above this. The images that DO declare are the
ones at risk, and they are the memory-tight ones.

## What a fix has to decide

Not "raise the term back to `RX_BUF`" — that gives up issue 1255's saving on
the path where the per-type bound IS what is allocated. The question is which
of these:

* make the schemaless arm's default match the model — i.e. let a Rust typed
  registration on zenoh/XRCE reach the type's bound the way the C++ side does
  through `rx_size_bound<M>`. The bound exists (codegen emits it); what is
  missing is a way to read it at a site whose `MessageForRmw` has no schema;
* have the build model the REGISTRATION PATH, which it cannot see today;
* state the shortfall as a per-image margin the derivation adds, which is the
  answer that goes stale next time a path changes.

## Reproduction

Not reproduced as a failing image — the analysis is from the source
(`rmw_type_registry.rs::default_subscription_rx_bytes`, `spin.rs`'s
`register_subscription_buffered_on` and `add_arena_subscription_c_callback`).
An image that declares its entities, subscribes from RUST on zenoh, and sets
`NROS_SUBSCRIBER_BUFFER_SIZE` below `NROS_SUBSCRIPTION_BUFFER_SIZE` is the
shape that would show it, as `NodeError::BufferTooSmall` at a registration the
arena oracle passed.

*(Done — phase-454 W5. See the Resolution at the top: that exact image is what
corrected the third row of the table above.)*
