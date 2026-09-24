---
id: 1450
title: "The `[params] max_parameters` docs say the parameter store is PER NODE
  and the field is a maximum; the arena is SHARED and the producer SUMS — the
  consumer's own doc says so, and the two halves of one fact disagree in
  writing"
status: resolved
type: tech-debt
area: [core, cli, tooling]
severity: medium
found: 2026-09-22
related: [1408, 1436, 1429, 0196]
---

## What is open

`max_parameters` is documented in three places. **The code is right everywhere;
two of the three docs are wrong**, and they are wrong about the architecture
rather than about a number — they describe a per-node parameter store that does
not exist.

| site | says | correct? |
| --- | --- | --- |
| `nros-params/src/server.rs:476` | "the arena is SHARED across nodes, so this is not a per-node budget: `MAX_PARAMETERS` bounds the image, not each node" | **yes** |
| `nros-sizing-descriptor/src/schema.rs:752` | "PER NODE, because the store is per node" | no |
| `nros-sizing-descriptor/src/schema.rs:814` | "the maximum over the image's nodes and not the sum" | no |
| `nros-cli-core/src/entity_inventory.rs:1455` | "`NROS_MAX_PARAMETERS` — per node, the declared names plus `SEEDED_PARAMETER`" | no |

## The store is one flat table

```rust
// packages/core/nros-params/src/server.rs
pub struct ParameterStorage<const N: usize = MAX_PARAMETERS> {
    entries: [Option<ParameterEntry>; N],
}
```

One array for the whole image, and each `ParameterEntry` carries its own `node`.
`is_full()` is documented "across every node". `capacity()` carries phase-426
W1's note in full:

> the arena is SHARED across nodes, so this is not a per-node budget:
> `MAX_PARAMETERS` bounds the image, not each node.

## The producer sums, which is correct

```rust
// packages/cli/nros-cli-core/src/entity_inventory.rs
let mut max_parameters = 0usize;
for node in nodes {
    let names: BTreeSet<&str> = /* this node's declared names + the seed */;
    max_parameters += names.len();          // SUM, not max
    for n in names {
        max_param_name_len = max_param_name_len.max(n.len());   // its sibling DOES max
    }
}
```

A shared table must hold every node's parameters at once, so the sum is the
right number and the `+=` beside a `.max()` is deliberate, not a slip. Measured
on `tests/fixtures/param_declarations/`: two nodes, three declared parameters,
`max_parameters` = 5 (3 declared + 2 seeded `use_sim_time`).

## Why this is worth an issue rather than a typo fix

The descriptor doc does not merely mislabel the field — **it warns the reader
away from the correct derivation**:

> Per node rather than per image because the store is per node, so this is the
> maximum over the image's nodes and not the sum. A consumer that summed
> `Self::declared` instead would size every node's store for the whole image.

Every clause is false, and the last one inverts the hazard. Someone reconciling
the producer to that doc would replace `+=` with `.max()`, and an image whose
parameters are spread across nodes would then get a table too small to hold
them: `declare_parameter` returns `SetParameterResult::StorageFull` at boot, per
node, in declaration order. That is the 1015/1033 family — a derived count that
is correct at one layer and defeated at another — with the doc as the thing that
defeats it.

It is also the shape issue 1436 just closed one field over: two halves of one
fact maintained separately, agreeing by accident until someone reads only one.

## Not a live defect

Nothing ships mis-sized today. The producer sums, the consumer sizes a shared
table from the sum, and `nros-params/build.rs` passes the number straight
through as `N`. The cost is entirely in what the next reader is told.

## What closing it looks like

1. Correct the two descriptor doc comments (`schema.rs:752`, `:814`) and the
   inventory's (`entity_inventory.rs:1455`) to say IMAGE-WIDE, and say why —
   the arena is shared, so the bound is a sum over nodes.
2. Say it once. The right place is beside the code that owns the fact; the other
   sites should point at it rather than restate it, which is what let three
   spellings drift in the first place.
3. Consider whether the FIELD NAME earns its keep. `max_parameters` reads as a
   maximum and is a sum; `max_param_name_len` beside it really is a maximum.
   Renaming reaches the carrier `NROS_MAX_PARAMETERS`, the Kconfig
   `CONFIG_NROS_MAX_PARAMETERS` and a descriptor field, so it is not free — but
   the name is half of why the doc went wrong.

No gate is proposed. The claim is prose about an architecture, and the sweep
that found it (`grep -n 'per node' | grep MAX_PARAMETERS`) is not a rule a
checker can hold without flagging the correct site too.

## How it was found

Reported during phase-454's `[params]` slice (issue 1408) by the agent that
wired the consumer: it carried the producer's number faithfully, noticed the
schema doc disagreed, and changed neither side. Verified here against the code
on `main` before filing.


---

# RESOLVED — the docs move, the name stays, and the reason is that `max` was never the wrong word

## What changed

Three sites said PER NODE and are now IMAGE-WIDE, each giving the reason rather
than restating the rule:

| site | now |
| --- | --- |
| `nros-params/build.rs` (the generated `MAX_PARAMETERS` doc) | **the canonical statement** — one flat array, each entry carrying its node, so the bound is over every node's parameters taken together, and the contract's number is a SUM |
| `nros-sizing-descriptor/src/schema.rs` (`Params` struct doc) | IMAGE-WIDE, pointing at `nros_params::MAX_PARAMETERS`; records that it said the opposite |
| `nros-sizing-descriptor/src/schema.rs` (`max_parameters` accessor) | the capacity, summed, pointing at the same place; the false "maximum not the sum" paragraph is gone |
| `nros-cli-core/src/entity_inventory.rs` (`ParamStoreSizing`) | IMAGE-WIDE, and says why the `+=` sits beside `max_param_name_len`'s `.max()` |

**Said once.** The canonical statement lives on the generated const, because that
is the doc every consumer's rustdoc shows and the crate that owns the store. The
other three point at it. Restating it in three places is what let three spellings
drift.

## Two sites the sweep found and did NOT change

Both say "per node" and both are CORRECT, about a different fact:

* `nros-node/src/parameter_services.rs` — the seeded `use_sim_time` placeholder
  is per node, "because the store is **keyed** by node and the six services are
  published per node". Keyed by node is true; that is not the same claim as the
  store being per node.
* `nros/src/node.rs` (twice) — `param_node` exists because the services are
  registered per node "and the store is keyed the same way".

Recording these matters as much as the fixes: the wrong claim and the right one
are one word apart (*keyed by* vs *per*), and a future sweep that greps
`per node` will land on all five.

## The rename: considered, and DECLINED

The issue asked whether `max_parameters` earns a name that reads as a maximum
and is a sum. It does, and the framing was the error:

**`max` names the CAPACITY — the most the store can hold — not a maximum taken
over nodes.** `MAX_PARAM_NAME_LEN`, `MAX_STRING_VALUE_LEN`, `MAX_ARRAY_LEN` and
`MAX_BYTE_ARRAY_LEN` all read that way already, and nobody has ever read those
as "the maximum over the image's nodes". The derivation sums DEMAND to reach a
required capacity, which is what any capacity knob does.

So the defect was never the name. It was a doc that imported a per-node store
model and then had to explain the sum away — and, having invented the model, it
warned the next reader against the correct derivation. Renaming would have
reached `NROS_MAX_PARAMETERS`, `CONFIG_NROS_MAX_PARAMETERS`, a descriptor field
and the book, to fix prose.

## Verification

`cargo clippy -p nros-params -p nros-sizing-descriptor --all-targets -D warnings`
clean; `nros-sizing-descriptor` 49/49 and `nros-cli-core` param tests 36/36. The
generated doc was read back off disk rather than assumed — it is a Rust string
emitting Rust doc comments, and the escaping is the part that breaks silently.

Still true, and still the point: nothing shipped mis-sized. The producer summed,
the consumer sized the shared table from the sum. This wave changed only what
the next reader is told.
