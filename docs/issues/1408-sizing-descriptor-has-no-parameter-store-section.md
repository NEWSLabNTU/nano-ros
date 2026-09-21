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
