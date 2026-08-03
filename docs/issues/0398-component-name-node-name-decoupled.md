---
id: 398
title: "`[[component]] name` no longer matches the launch node name, so every
  per-node projection keyed on it silently does nothing"
status: open
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
