---
id: 1374
title: "A contract's `qos:` carries depth and nothing else, so a durability the
  code depends on is invisible to the declaration that is otherwise the truth"
status: open
type: bug
area: contract, qos, orchestration-ir
severity: medium
related: [issue-1352, issue-1372]
---

## What happens

A launch contract declares a subscription's QoS as

```yaml
operation_mode_state: { min_rate_hz: 10, qos: { depth: 1 } }
```

and `depth` is the only policy that survives. The resolved model confirms it --
`build/nros/models/<bringup>/system_model.yaml` carries

```yaml
    /mrm_handler/operation_mode_state:
      min_rate_hz: 10.0
      qos:
        depth: 1
```

for every subscription in the image, and **no publisher entry carries a `qos`
block at all.**

Meanwhile the code those entries describe sets durability, and depends on it.
On Autoware Safety Island, five sites:

| site | policy |
| --- | --- |
| `mrm_handler_core.cpp:104` sub `/api/operation_mode/state` | `QoS(1).transient_local()` |
| `mrm_comfortable_stop_operator_core.cpp:59` pub | `QoS(1).transient_local()` |
| `mrm_comfortable_stop_operator_core.cpp:63` pub | `QoS(1).transient_local()` |
| `stop_mode_operator.cpp:54` pub gear / turn / hazard | `QoS(1).transient_local()` |
| `stop_mode_operator.cpp:53` pub control | `QoS(5)` -- depth 5, not 1 |

The first one carries a comment saying exactly why it is load-bearing:

```cpp
// transient_local: the topic is latched at source; a volatile sub misses
```

So a policy that decides whether a node sees a latched topic at all is chosen in
code, and the declaration that the rest of the build treats as authoritative
cannot say it.

## Why this is worse than a missing field

Depth is not merely recorded -- it is **enforced**. Codegen emits the contract's
declared QoS into `<nros/nros_declared_qos_generated.h>` and `NROS_SUBSCRIBE`
static-asserts the code's depth against it, so a contract that disagrees with
its source is a build error naming the topic and both numbers.

Durability has no equivalent. There is nothing to disagree with, so the two can
diverge indefinitely and no build, gate or boot check will say so. The
asymmetry is the defect: one policy in the same `qos:` block is checked to a
compile error, and the others are not expressible.

The same block also drops the depth-5 publisher above. `stop_mode_operator`'s
control publisher is `QoS(5)`, and the contract has no publisher `qos:` to
record it in.

## Durability is not missing from the tree, only from the contract

`packages/core/nros-orchestration-ir/src/qos_override.rs:108` already names the
full policy set:

```
reliability, durability, history, depth, deadline, lifespan, ...
```

reached as `qos_overrides.<topic>.<endpoint>.<policy>` parameters. That is the
ROS 2 runtime override path -- a parameter, resolved per deployment -- and it is
the right home for a deployment tweak. It is not a substitute for the contract
stating what the code requires, for two reasons: an override is optional and
absent by default, and it is a *parameter*, so on an image with the parameter
services disabled it is not reachable at all.

## What a fix has to decide

Not a pure addition. Three questions the design has to answer:

1. **Publishers have no `qos:` block today.** Adding durability means adding
   that block, which changes the contract schema for every image.
2. **Which policies are contract facts and which are deployment policy?**
   `depth` and `durability` look like facts about what the code requires.
   `deadline` and `lifespan` look like deployment policy. `reliability` is
   arguably either. Drawing that line is the actual work; putting all six in
   the contract would duplicate the override mechanism.
3. **Does a declared durability get enforced the way depth does?** If yes, the
   generated-header static assert has to grow; if no, the new field is
   documentation and the asymmetry above just moves.

## Measured on

Autoware Safety Island, 4 nodes, 11 subscriptions and 14 publishers, all 11
subscriptions declaring `qos: { depth: 1 }` and none declaring durability,
against five `transient_local` sites and one depth-5 publisher in the source.
