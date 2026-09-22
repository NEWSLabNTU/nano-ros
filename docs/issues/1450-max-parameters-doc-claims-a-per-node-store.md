---
id: 1450
title: "The `[params] max_parameters` docs say the parameter store is PER NODE
  and the field is a maximum; the arena is SHARED and the producer SUMS — the
  consumer's own doc says so, and the two halves of one fact disagree in
  writing"
status: open
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
