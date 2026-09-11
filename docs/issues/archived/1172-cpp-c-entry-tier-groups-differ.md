---
id: 1172
title: The C and C++ entry emitters derive a tier's callback groups differently
status: resolved
area: codegen
severity: medium
opened: 2026-09-06
---

# The C and C++ entry emitters derive a tier's callback groups differently

Both entry emitters bake a per-tier array of callback-group names, and the
runtime uses those arrays to decide which tier runs a callback. The two
emitters build the array from the same `ResolvedTierTable` and do not agree.

`emit_c` (`packages/cli/nros-cli-core/src/codegen/entry/emit_c.rs`):

```rust
// deduped ACROSS tiers, empty names dropped
let mut seen: HashSet<String> = HashSet::new();
for t in &tiers.tiers {
    for (_, grp) in &t.members {
        if !grp.is_empty() && seen.insert(grp.clone()) { g.push(grp.clone()); }
    }
}
```

`emit_cpp`:

```rust
// deduped WITHIN each tier, empty names kept
let mut seen = BTreeSet::new();
tier.members.iter().filter_map(|(_, g)| seen.insert(g.clone()).then(|| g.clone()))
```

Two consequences, both silent:

1. **A group named by two tiers.** C lists it under the FIRST tier only; C++
   lists it under BOTH. So the same plan produces a group routed to one tier in
   a C entry and present in two tier arrays in a C++ entry.
2. **A member with an empty group name.** C drops it; C++ emits `""` into the
   array and counts it in `n_groups`, so the C++ tier claims a group whose name
   is the empty string.

Neither shows up today because no golden and no fixture declares a plan with a
cross-tier group or an empty group name — which is exactly why it survived.

## Why it is open rather than fixed

Found while converting `emit_cpp` onto a template (phase-432 W2.3), which is a
byte-for-byte change: reconciling the two moves goldens, so it belongs in its
own commit with its own diff to read. The shared row type
(`codegen::entry::TierView`, added in W2.3) is what makes the divergence
visible at all — before it, the two derivations sat in two files with no
common declaration.

## What a fix has to decide

Which behaviour is correct is a RUNTIME question, not a style one: it depends
on what the tier arrays mean to `run_tiers` when a group appears twice. Answer
that first, then make it one derivation beside `tier_views`, and add a golden
row that carries a cross-tier group and an empty group name so the answer is
pinned.

## Scope — option 4, node-qualified filtering (2026-09-08)

The chosen fix is not "pick C's rule or C++'s". Both are wrong for the same
reason: **the filter matches on group NAME alone**, so it cannot tell node A's
`ctrl` from node B's `ctrl`, and the codegen difference is only how each pack
copes with an ambiguity neither can express.

### What the filter actually is

`Executor::group_active` has ONE caller in the tree — `create_entity`
(`packages/api/nros/src/node_runtime.rs:1338`). It is a REGISTRATION gate, not
a dispatch gate: an entity whose group is inactive on this tier is never
created, so it gets no RMW handle and no slot. `metadata.node_id` is already in
scope there (used three lines below to `lookup_node`), so the node identity is
available at the exact point the match happens.

### The precedent to copy

`Executor::bind_group_sched` (`spin.rs:2129`) already solves this for the
sched-context path: it keys `group_sched_table` on the TUPLE
`(name<64>, ns<64>, group<32>)` and matches all three. The tier filter is the
same question with a different answer shape, and it should key the same way —
a tuple, not a delimiter-joined string, so no name can collide by containing
the delimiter.

That the two paths already disagree is itself the finding: `bind_group_sched`
is node-qualified and `set_active_groups` is not, and both are fed by the same
emitter from the same `tier.members`.

### What changes

RUNTIME (`nros-node/src/executor/spin.rs`)
  * `active_groups: CarvedVec<GroupName>` becomes a tuple vec keyed like
    `group_sched_table`.
  * `set_active_groups(&[&str])` takes triples.
  * `group_active(group)` becomes `group_active(name, ns, group)`.
  * `create_entity` moves its `lookup_node` ABOVE the gate and passes the
    node's name and namespace.

C ABI (`nros_cpp_executor_set_active_groups`)
  * The signature does NOT change. The array becomes 3N strings — name, ns,
    group, repeated — and the count stays the number of ENTRIES. That is what
    keeps `nros_native_tier_spec_t` byte-identical: no change to the struct,
    its 8 mirrors, the designated initialisers or the C runners, which pass the
    array through blind.
  * The cost of that choice, stated: `groups` / `n_groups` then name a triple
    array, so the field names under-describe the content. The alternative —
    three parallel arrays or a struct — is an ABI change across 8 mirrors and
    7 runners, which is a phase rather than a wave.

CODEGEN (both entry packs)
  * `groups_per_tier` emits the triple, and the two derivations collapse: with
    the node in the key there is nothing to dedup ACROSS tiers, so C's rule and
    C++'s rule stop differing. `tier_views`' `groups_per_tier` parameter — the
    one this issue exists to remove — goes away.

### What this changes for a user

An observable behaviour change, and no fixture covers it today. A tier filter
that matched any node's group of that name will now match only the named
node's. A plan relying on the old breadth — deliberately or not — behaves
differently.

### Acceptance

A RUNTIME test, not a golden: a tiered fixture with two nodes declaring the
SAME group id pinned to different tiers, asserting each tier registers only its
own node's entities. Everything in this issue is read from source; the case has
been unreachable by construction, which is why it survived.

## Fixed — option 4, variant C (2026-09-08)

The filter key is now the TRIPLE `(node name, node namespace, group)`, all the
way down: `nros-node`'s matcher, the `TierSpec`/`nros_native_tier_spec_t` seam,
the `nros_cpp_executor_set_active_groups` FFI, the `nros::main!` macro and both
entry packs.

**Why that dissolves the disagreement rather than picking a winner.** The two
rules were answering "what happens when two tiers name the same group?", and
neither answer was defensible because the filter could not express the node.
With the node in the key that question does not arise — two nodes' `ctrl` are
two different keys — so there is nothing to dedup across tiers and the one rule
left is the same for both packs. It lives in `codegen::entry::tier_group_keys`
and both emitters call it.

**The C behaviour was worse than the issue text said.** The text repeated the
`emit_c` comment's claim that "a group named by two tiers belongs to the
first". That is not what the code did: deduping across tiers left the SECOND
tier's array EMPTY, and an empty array is the WILDCARD (`main.h`: "NULL / 0
means wildcard"), so that tier stopped filtering and ran every callback in the
image at its own priority. C failed OPEN; C++ failed closed.

**Wire shape.** `groups` is FLAT — 3 × `n_groups` null-terminated strings, and
`n_groups` counts TRIPLES. Flat rather than an array of structs so
`nros_native_tier_spec_t` stays byte-identical: its eight hand-written mirrors,
the designated initialisers and the three C tier runners pass the array through
without reading it, so none of them changed. Rust holds the same data as a
tuple slice.

**Capacity and silent drops, fixed in the same change** (they are the same
defect one layer over — a filter quietly narrower than the one it was given):

- `nros_cpp_executor_set_active_groups` read `n.min(MAX_GROUPS_FFI)` and its
  comment said "silently truncates extras". A 17th group was dropped before the
  executor saw it. It now refuses with `NROS_CPP_RET_FULL`.
- `Executor::set_active_groups` fails CLOSED on overflow: it clears the list,
  leaves filtering ON, and returns `Err` — so an executor that cannot express
  its filter registers nothing rather than everything.
- `n_groups > 0` with every group id empty used to collapse to the empty slice,
  i.e. the wildcard. It is now `NROS_CPP_RET_INVALID_ARGUMENT`.
- `bind_node_name_sched` / `bind_group_sched` return `Result` and report
  `NROS_CPP_RET_FULL` instead of dropping a bind.

**What pins it.** `tier_group_keys` has a unit test built on the shape the
corpus never had — one group id (`ctrl`) on two nodes, on two tiers, one of
them namespaced. Mutation-checked against BOTH old rules: keying on the group
alone trips the "keys must differ" assertion, deduping across tiers trips the
"tier 1 must not be empty" one. On the runtime side,
`the_same_group_on_another_node_is_not_accepted`,
`the_same_name_under_another_namespace_is_a_different_node`,
`an_empty_namespace_normalises_to_root`, `the_name_namespace_split_cannot_be_forged`
and `an_oversized_group_name_is_refused_and_fails_closed`.

The acceptance test this issue asked for — a tiered FIXTURE with two nodes
sharing a group id — is not in this change. The behaviour is pinned by the unit
tests above at both layers; a fixture costs a matrix cell and belongs with the
tier fixture work, so it is called out here rather than quietly dropped.

## Related

- phase-432 (`docs/roadmap/archived/phase-432-codegen-one-producer-many-packs.md`) — W2.3.
- RFC-0091 — one entry-codegen producer, many language packs.
- RFC-0047 — callback groups and tiers.
