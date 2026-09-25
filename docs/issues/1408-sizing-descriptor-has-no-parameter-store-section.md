---
id: 1408
title: "The sizing descriptor's schema has no parameter-store section, so the
  nine parameter-shape facts have no descriptor spelling on ANY road — they are
  out of RFC-0100 D4's vocabulary, not refused by a producer"
status: open
type: tech-debt
area: [build, core]
related: [1393, 1407]
found: 2026-09-21
---

## What is open

RFC-0100 D4's schema has five sections — `[meta]`, `[target]`, `[[endpoint]]`,
`[image]`, `[types]`, `[policy]`. None of them can hold a parameter.

The parameter store is nevertheless DERIVED from the contract, by the same
inventory everything else in phase-454 comes from
(`ParamDeclarations::from_model`, phase-446 W4), and it travels to consumers as
nine `NROS_DECLARED_*` carriers:

| carrier | reader |
| --- | --- |
| `NROS_DECLARED_MAX_PARAMETERS` | `packages/core/nros-params/build.rs` |
| `NROS_DECLARED_MAX_PARAM_NAME_LEN` | ditto |
| `NROS_DECLARED_MAX_STRING_VALUE_LEN` | ditto |
| `NROS_DECLARED_MAX_ARRAY_LEN` | ditto |
| `NROS_DECLARED_MAX_BYTE_ARRAY_LEN` | ditto |
| `NROS_DECLARED_PARAM_NEEDS_MAX_STRING_VALUE_LEN` | ditto |
| `NROS_DECLARED_PARAM_NEEDS_MAX_ARRAY_LEN` | ditto |
| `NROS_DECLARED_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN` | ditto |
| `NROS_DECLARED_PARAM_SERVICE_SHAPE` | `packages/core/nros-node/build.rs` |

## Why this is a different shape from 1393

[Issue 1393](1393-cmake-road-has-no-bound-inventory.md) and
[issue 1407](1407-cmake-road-descriptor-coverage-narrower-than-its-carriers.md)
are both about a PRODUCER that cannot source a field the schema HAS. This is the
opposite: the producer has the fact — `ParamDeclarations` is composed on the
leaf road and the model road alike — and there is nowhere in the file to put it.

So the asymmetry that governs the other two does not apply here. The parameter
facts are equally unstateable on **all three** roads, including the
single-package cargo leaf that states everything else, and a `Fact::Refused`
cannot even be written for them: `refuse()` rejects a key the section does not
have, which is one of the three rules enforced at PARSE (*"a refusal nobody can
read is worse than none"*).

## What CLOSING it looks like

A `[params]` section on the D4 schema — an RFC amendment, not a bug fix, and
therefore a decision rather than plumbing. Four questions it would have to
answer, none of which the existing sections settle:

1. **Per image or per node?** `ParamDeclarations::from_model` REFUSES unless
   EVERY node in the image declares `params:`, because the store holds every
   node's parameters and sizing from the nodes that did declare gives the rest
   no slots. `[[endpoint]]` is per endpoint and `[image]` is per image; a
   parameter capacity is per image derived over nodes, which is a third shape.
2. **Capacities or declarations?** Five of the nine are capacities
   (`MAX_ARRAY_LEN`), three are the raw per-declaration needs the consumer
   maxes, and one (`PARAM_SERVICE_SHAPE`) is a SHAPE token rather than a number.
   D7 says derivation publishes demand unfloored, which argues for the needs;
   the consumers today read both.
3. **Does it move `[meta] basis`?** Every consumer guards on `basis`, and a
   parameter-only contract describes no wiring — `EntityInventory::from_model`
   returns `None` for it while `ParamDeclarations::from_model` returns a real
   answer (the two predicates already differ, `cmd/entity_inventory.rs:259-265`
   attaches the parameter half *"whether or not the model describes wiring"*).
   So a descriptor written for parameters alone is a file whose endpoint table
   is empty on purpose, which W14's "no contract, no file" rule currently
   forbids.
4. **Does `nros-params` gain a descriptor read?** It has none today; the whole
   crate is on the carrier.

## Until then

The nine carriers STAY, and they are registered as KEPT against this issue in
`scripts/check/check-knob-single-reader.py`. Retiring any of them would leave
`nros-params` and `nros-node` reading crate defaults with nothing on any road to
correct them — "absence is not zero" with no replacement at all, which is
strictly worse than the other two blockers, where at least one road answers.

## What has LANDED (phase-454, RFC-0100 D4)

The vertical slice: the section, the composer that fills it and the consumers
that read it. **The issue stays OPEN for the retirement alone** — see the last
section.

The four questions above, answered:

1. **Per image or per node?** IMAGE-WIDE, every field. There is no partial
   per-node state to model, because `ParamDeclarations::from_model` refuses
   outright unless every node declares — so the section is one table and not a
   row per node.

   **This answer said "the store ... is per node" until issue 1450 measured it,
   and that was wrong.** `ParameterStorage<const N>` is ONE flat
   `[Option<ParameterEntry>; N]` for the whole image, each entry carrying its own
   node; `ParameterServer::capacity` has said so since phase-426 W1 ("the arena
   is SHARED across nodes, so this is not a per-node budget: `MAX_PARAMETERS`
   bounds the image, not each node"). So `max_parameters` is a CAPACITY the
   producer SUMS over nodes, `max_param_name_len` is the longest single name
   image-wide, and `declared` differs from `max_parameters` only by the per-node
   seeded `use_sim_time`. `nros_params::MAX_PARAMETERS` carries the canonical
   statement.
2. **Capacities or declarations?** NEITHER, for the three capacities: they are
   RFC-0100 D1 **target** facts owned by `[board.knobs.params]` (an MCU and a PC
   want different string lengths for the same node), so the schema deliberately
   does NOT carry them. It carries the NEED — `CapacityNeed`, whose `Unused` arm
   is a STATEMENT carried by `Fact::Stated` and distinct from an absent key.
   `service_shape` stays the TOKEN, because re-spelling its nine counts as TOML
   would make the schema crate the second author of a grammar it does not own
   (issue 1025's defect).
3. **Does it move `[meta] basis`?** No, and it does not move `[meta] status`
   either. `overall_status` folds `[target] pointer_bytes` and each endpoint's
   `wire_bound_bytes` / `registration_path` — facts EVERY image has — and
   `[params]` is not like that: it is ABSENT for the overwhelming majority of
   images and REFUSED only for one that half-declared. Folding `Absent` in would
   make every descriptor in the tree read `partial` for a section nobody filled;
   folding `Refused` in would let one image's half-authored `params:` cost every
   unrelated derivation in the file its summary. The argument is written out at
   `overall_status` in `packages/cli/nros-cli-core/src/sizing_descriptor.rs`.
4. **Does `nros-params` gain a descriptor read?** Yes — `from_build_env()`, at
   the rung the carrier occupied, with the carrier read BELOW it.

Measured, four builds of `nros-params` + `nros-node` differing only in
`NROS_SIZING_DESCRIPTOR`:

| descriptor | `MAX_PARAMETERS` | `MAX_PARAM_NAME_LEN` | `DECLARED_PARAM_SERVICE_SHAPES` |
| --- | --- | --- | --- |
| none | 32 | 64 | `None` |
| `[params]` empty (`Absent`) | 32 | 64 | `None` |
| `[params]` fully REFUSED | 32 | 64 | `None` |
| `[params]` STATED | **5** | **15** | `Some(&[[3,24,0,0,1,1,0,0,0],[2,27,0,0,0,0,0,0,0]])` |

The first three rows are BYTE-IDENTICAL `nros_params_config.rs`, which is the
control: a refusal is never a value. And the STATED row is byte-identical to the
same contract delivered by the nine env carriers, which is the parity proof —
the two roads are one derivation.

A declared `string` with no board capacity still REFUSES the build, loudly and
by name, on the descriptor road exactly as on the carrier road.

## What is STILL open

The retirement. A carrier comes out only once both roads are measured delivering
on every road that carries it, and three roads have no descriptor at all today:

* a STANDALONE cargo leaf with no resolved model (issue 1407's `_LEAF_ROAD`
  shape — no model, so no `write_for_model`);
* a MULTI-ENTRY cmake configure, which names no descriptor to cargo;
* the Zephyr west lane, which has no `--config` seam of its own (issue 1288).

So the nine stay KEPT, with their ledger reasons rewritten to say this rather
than "the schema has no parameter section", which is no longer true. The three
BOARD capacities are a separate case and will never retire into the descriptor —
they are D1 target facts by design.

There is one adjacent gap this slice did NOT close, and it is being fixed on its
own branch rather than here: a **parameter-only** contract (no topics, services,
actions or node paths) reaches no descriptor on the cargo road at all, because
`EntityInventory::from_model` returns `None` for a model that describes no wiring
and the road nests the `ParamDeclarations` attach inside that `Some` arm — so a
contract that declares only parameters loses every parameter fact. Nothing in
this slice depends on it: every road that already writes a descriptor now carries
`[params]`, and that one adds a road that did not.
