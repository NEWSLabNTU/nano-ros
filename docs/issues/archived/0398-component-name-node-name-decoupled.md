---
id: 398
title: "`[[component]] name` no longer matches the launch node name, so every
  per-node projection keyed on it silently does nothing"
status: resolved  # fixed 2026-08-05
type: bug
area: orchestration
related: [phase-331, phase-330, rfc-0066, rfc-0047, 0380]
---

## Problem

`system.toml` binds a `[[component]]` to a launch node by BARE NAME: the
resolver takes the component's `name` and looks for a node whose FQN ends with
it. `[[component]].group_tiers` (RFC-0047 W2) has always used that rule.

The phase-331 consolidation broke the assumption it rests on. Merging the themed
workspaces into `features/` required component names to be unique across the
whole bringup, so they gained language and feature prefixes — while the launch
files kept the plain node name:

```toml
[[component]]
pkg   = "rust_param_talker_pkg"
name  = "rust_params_param_talker"     # workspace-unique instance id
```

```xml
<node pkg="rust_param_talker_pkg" exec="param_talker" name="param_talker"/>
```

**Zero of `features/`'s 20 component names match any of its 8 launch node
names.** Measured, not estimated:

```
components: 20   launch nodes: 8
names in BOTH: NONE
```

## Why it has not bitten yet

`features/` declares no `group_tiers`, so until now nothing in that bringup used
a per-node projection. The failure surfaced only when phase-330 W4 added the
second one (`[[component]] params` / `params_files`): the declarations were
correct, the resolver ran, and the values never reached the model.

It was visible at all only because that projection emits a diagnostic on an
unmatched component. A projection that silently skips — which is what
`group_tiers` does — would have left no trace.

## Current state

Worked around in `ros-launch-manifest` v0.1.2: when the bare name does not
match, `apply_params_to_nodes` falls back to the component's PACKAGE, and only
when that package is unambiguous (two instances of one package are exactly what
the instance name exists to disambiguate). That rescues the params case.

**`group_tiers` still uses bare-name matching only.** The moment `features/`
declares one — or any consolidated workspace does — it will bind nothing, and
tiers will silently fall back to the default with no diagnostic.

## Direction

Pick one; both are defensible, and the choice belongs with phase-331 since it
owns the naming:

1. **Recouple the names.** `[[component]] name` goes back to the launch node
   name, and uniqueness comes from namespacing (`<group ns=…>`) rather than from
   the identifier. Matching stays trivial and one rule covers every projection.
2. **Key the projections on something stable** — `pkg` + `class`, as v0.1.2 does
   for params — and treat `name` as a display/instance id only. Then
   `group_tiers` must move to the same key, or the two projections disagree
   about what a component IS.

Either way, the invariant worth having is that ONE rule matches components to
nodes, and that failing to match is loud. The bare-name rule is currently
implicit, undocumented, and silent on failure — which is why a whole-workspace
rename could invalidate it without a single test noticing.

## Resolution (2026-08-05) — direction 2, plus the loud failure

**Decision (phase-331 owns the naming): `[[component]].name` stays an INSTANCE
ID.** Recoupling it to the launch node name (direction 1) would mean giving four
language variants of one role distinct namespaces, which changes wire-visible
node names and every test that asserts them — a large blast radius to buy a
matching rule that is already worked around. Projections key on the documented
rule (bare name, then unambiguous package, as rlm v0.1.2 does for params).

What was missing is the invariant this issue actually asks for: **failing to
match is loud.** The model->component direction already bails on a binding that
names no component. The reverse was silent — a `[[component]]` declaring
`group_tiers` that matched no node produced no binding, so there was nothing to
reject and the node ran on the default tier.

`apply_model_execution` now refuses that. The check distinguishes the two cases
that look identical from the outside:

  * **absent in this variant** — no node of the component's PACKAGE is in this
    model. Legitimate and common: a bringup is a catalog, and each launch uses a
    subset (`realtime-cpp`'s `aux_node` is in the freertos launch, not the
    native one). Not an error, or every multi-variant bringup becomes unbakeable.
  * **renamed** — a node of the SAME package IS in the model under another name.
    That is a component whose node is right there and did not match: the
    phase-331 shape, and an error naming the component, its package and the node
    it should have bound to.

Watched both directions rather than assumed: `workspace-cpp-native-realtime`
builds clean with `aux_node` absent, and renaming `ctrl_node` to
`renamed_ctrl_node` fails with

    these `[[component]]`s declare `group_tiers` that reached no node, while a
    node of the SAME PACKAGE is in the resolved model: `renamed_ctrl_node`
    (its package `ctrl_pkg` runs in this model as `/ctrl_node`)

The first draft of the check failed on absent-in-variant too, which the realtime
workspace caught immediately — worth recording, because "declaration reached
nothing" and "declaration does not apply here" are the same observation until
you look at the package.
