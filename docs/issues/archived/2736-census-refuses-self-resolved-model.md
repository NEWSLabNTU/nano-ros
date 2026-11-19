---
id: 2736
title: "`model_gate::verify` refused a model `ensure_model` resolved for itself, which `model_gate::open` accepted"
status: resolved
type: bug
severity: low
area: [cli, orchestration]
found: 2026-11-19
resolved: 2026-11-19
resolved_in: "fix(cli, issue 2736): one rule for a self-resolved model, in model_gate::verify"
related: [2706, 2735]
---

## What happened

One rule, spelled twice. `ensure_model` resolves a model into the user cache
when no build has produced one, and such a model records the resolver's own
version (`0.9.0`), not the pin `nros sync` stamps, because it is current by
construction. `model_gate::open` exempted it from the pin check; its comment
records `build_verb_pipeline` going red on the same `0.9.0`-against-our-pin.
`model_gate::verify` did not, and the census check (`check_census`, called
from `check_cargo_image`) calls `verify`, with no bringup.

Reproduction: a leaf that has a launch but no model sync produced. Met
while building issue 2706's Zephyr interrupt-wake image. A `[[component]]`
row was added, then removed, which left `build/nros/launch/.../system.launch.xml`
behind. Every `nros configure` then printed

```
SystemModel `~/.cache/nano-ros/models/<hash>-<leaf>/system_model.yaml`
is stale: resolver pin changed (model `0.9.0` != ours `eb8758ae2260`)
  Run `nros configure` to re-resolve it.
```

and rewrote and refused the same model again, so the instruction could not
help.

Not every component-less leaf: on clean main a fresh such leaf takes another
path. Issue 2735's `leaf_sized_without_model` (a census-sized leaf) and the
older `owned_entry_without_model` (an image naming its own entry) route it
before any model is read, and anything else fails earlier, for a missing
launch.

## Fix

- `verify` decides whether the model is self-resolved
  (`model_location::is_self_resolved`, from the bringup it is given or
  infers) and skips only the pin check for one. The refusal marker still
  applies.
- `open` calls `verify` for every model it finds instead of keeping its own
  exemption, so the rule has one home.
- `check_census` passes the bringup it already holds (its `system.toml`'s
  directory), since a model in the user cache cannot have its bringup
  inferred from its path.

Other pin readers checked, none disagrees:
- `run_sync`'s `provenance_stale` checks models sync stamps itself.
- `verify_resolver_pin` checks the resolver binary.
- The configure stamp records the pin as a digest input.

Test: `model_gate::tests::a_self_resolved_model_is_not_refused_for_its_resolver_version`
checks that the same model with a `0.9.0` pin passes as self-resolved and is
refused otherwise.
