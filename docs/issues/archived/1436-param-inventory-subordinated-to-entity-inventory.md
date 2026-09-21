---
id: 1436
title: "The parameter inventory is a passenger on the entity inventory, so a
  contract declaring only `params:` loses every parameter fact on the cargo
  road — and fails the resolve naming wiring the user was never asked for"
status: resolved
type: bug
area: [build, cli]
severity: medium
found: 2026-09-21
related: [1408, 1393, 1407, 0196, 0973, 1228]
---

## What was wrong

Two inventories are composed from one resolved SystemModel, and they answer
INDEPENDENT questions:

* `EntityInventory::from_model` — what endpoints does this image create?
* `ParamDeclarations::from_model` — what parameters do its nodes declare?

The cmake road (`cmd::entity_inventory`) attaches them independently, and says
why in its own words:

> phase-446 W4 — the contract's `params:` size the parameter store. Attached
> whether or not the model describes wiring: **the two answers are
> independent, and a model with no topics can still declare parameters.**

The cargo road (`cmd::build`) nested the parameter half INSIDE the entity
half's `Some` arm:

```rust
EntityInventory::from_model(model_path.display().to_string(), &model)
    .map(|mut inv| {
        inv.set_param_declarations(ParamDeclarations::from_model(&model));
        inv                                   // ← only reached when Some
    })
    .ok_or_else(|| "the launch tree resolved, and it describes no wiring. …")
```

and `EntityInventory::from_model`'s predicate returned `None` for a model with
no topics, services, actions or `node_paths`. A contract declaring only
`params:` populates `contracts.node_params` and **none of those**, so it
answered `None`: the attach never ran, every parameter fact was discarded, and
the resolve failed with a message about wiring — for an image whose author had
declared parameters and nothing else.

## The predicate was answering a narrower question than it documented

Its own doc comment states the distinction it exists to preserve:

> Returns `None` when the model describes no wiring, so a caller cannot mistake
> **"nobody authored a contract"** for "this image creates nothing".

Those are two different predicates, and the code implemented the narrow one. A
contract that declares only `params:` **is** authored and **does** state a real
fact. `node_paths` was already in the test for exactly this reason — a
component whose only callback is a timer describes real wiring with zero
topics — and `node_params` belongs there on the same argument, one step
further out.

## Why nothing caught it

The repo's canonical parameter fixture is precisely this shape:

```yaml
# packages/cli/nros-cli-core/tests/fixtures/param_declarations/launch/declared.contract.yaml
nodes:
  a: { params: { rate: { type: integer } } }
  b: { params: {} }
```

No topics, no services, no actions, no paths. And the tests beside it call
`ParamDeclarations::from_model(&m)` **directly**, bypassing both roads'
composition — so they assert the inventory is right and never ask whether
either road carries it. The composer had two call sites, phase-446 W4 landed
the rule at one, and the fixture that would have shown it was routed around
the only code path that differed. The "fix the CLASS, not the reported site"
rule, failed at the site that had no test.

Blast radius is small and worth stating plainly: **no shipped example declares
`params:`** — zero of the six `*.contract.yaml` files in `examples/` — so
nothing in tree was broken. This is a defect a user meets, not one CI met.

## The fix

1. **The predicate asks its documented question.** `node_params` joins the four
   emptiness terms in `EntityInventory::from_model`. `None` now means "nobody
   authored a contract"; a parameter-only contract yields an inventory with
   zero entity rows, which is the TRUE answer for such an image rather than the
   absence of an answer.
2. **`cmd::build` composes `ParamDeclarations` before the entity inventory** and
   carries a `debug_assert!` pinning the implication that makes nesting the
   attach safe — `None` here implies the contract declared no parameters. If
   the predicate is ever narrowed again, that assertion fires at the site that
   would otherwise discard the facts, instead of the facts vanishing.
3. The error message on the genuine "nothing declared" path now names
   parameters as well as wiring, because both are now ways to answer it.

## The gate

`check-param-inventory-road-parity` (`just check param-inventory-road-parity`,
on the fast line via `just/check/codegen.just`). It checks the predicate covers
`node_params`, and that the road's backstop is in place. Buildless. Its
self-test is a negative control on the normal path (issue 1167 — a guard that
exists is not a guard that fires): it removes the `node_params` term and the
`debug_assert!` from copies and asserts each check goes red.

**It deliberately does NOT check "every site composing an `EntityInventory`
also composes `ParamDeclarations`".** That was the first shape, and the rule is
false: `contract_join` composes an inventory to join contract rows onto probe
rows and has no business with parameters, and the unit tests in
`entity_inventory.rs` compose dozens more. Written that way the gate reported
25 findings, 23 of them noise — a reach wider than the rule is how a real
finding gets scrolled past, the mirror of the issue-0196 shape.

## Regression test

`param_declarations_resolve::a_parameter_only_contract_is_still_an_authored_contract`
resolves the fixture through the pinned resolver and asserts the PREDICATE —
`from_model` is `Some`, the inventory is empty, and the parameter facts survive
composition. It also asserts the fixture is still parameter-only, so the test
cannot quietly stop covering this when the fixture grows wiring.

## What this does not close

[Issue 1408](../1408-sizing-descriptor-has-no-parameter-store-section.md) stays
open. The sizing descriptor still has no `[params]` section, so the nine
`NROS_DECLARED_*` parameter carriers are still the only transport for these
facts. This issue is why 1408's question 3 — *"does a parameter-only descriptor
move `[meta] basis`?"* — was never really a design question: `basis` describes
the ENTITY inventory, and the two inventories needed separating before that
could be seen.
