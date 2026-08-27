---
id: 829
title: "Two `SYSTEM_DEFAULT` QoS presets ship under one meaning and disagree on
  depth — 1 in `nros-rmw`, 10 in the `nros::qos` façade, each with two callers"
status: open
type: bug
area: api, rmw
related: [phase-379, issue-0160, issue-0088]
---

## Problem

The same profile is defined twice, with different depths:

| | value | callers |
| --- | --- | ---: |
| `QoSProfile::QOS_PROFILE_SYSTEM_DEFAULT` (`nros-rmw/src/traits.rs:751`) | Reliable, Volatile, KeepLast, **depth 1** | 2 |
| `nros::qos::SYSTEM_DEFAULT` (`api/nros/src/lib.rs:733`) | Reliable, Volatile, KeepLast, **depth 10** | 2 |

Depth is not cosmetic — it is how many samples the history queues before
dropping. A publisher created through the façade queues ten; the same profile
name reached through `QoSProfile` queues one.

Neither is a typo of the other: the façade's is `..DEFAULT` (depth 10) and the
`nros-rmw` one is an explicit `build(..., 1)`. Two people wrote two constants.

Found by a drift test added in phase-379 W5 while giving the presets their
rclrs-shaped crate-level names. The test was written expecting to *prove* the
two copies agreed; it failed on the first run.

## Neither matches upstream, which is the harder half

`rmw_qos_profile_system_default` does not name concrete policies at all:

```c
static const rmw_qos_profile_t rmw_qos_profile_system_default = {
  RMW_QOS_POLICY_HISTORY_SYSTEM_DEFAULT,
  RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT,
  RMW_QOS_POLICY_RELIABILITY_SYSTEM_DEFAULT,
  RMW_QOS_POLICY_DURABILITY_SYSTEM_DEFAULT,
  ...
};
```

Every field is a *sentinel* meaning "let the RMW decide". Ours are concrete on
both sides, so `SYSTEM_DEFAULT` currently means "a profile someone picked",
not "the implementation's own default" — which is what a ported ROS 2 node
reading the name will assume.

So there are two questions, and only the first is a bug fix:

1. **Which depth wins?** The two copies must not disagree.
2. **Should the profile mean what upstream means?** That needs a sentinel
   concept (`SYSTEM_DEFAULT` as a distinct policy value) which the QoS repr
   does not have, and which reaches the C ABI's `nros_qos_t`.

## Why it was not decided in passing

Both spellings have exactly two callers, so neither is dominant and there is no
"obviously the live one". Picking the wrong depth silently changes queueing
behaviour for whichever set of callers loses — the sort of change that surfaces
as a dropped-sample bug three phases later, not as a test failure.

## Current state, and the guard that now exists

`packages/api/nros/src/lib.rs` gained the eight crate-level `QOS_PROFILE_*`
consts (rclrs parity — the names always matched, only the path did not) as
ALIASES of the `QoSProfile` associated consts. A test, `qos_preset_parity`,
asserts the façade's `nros::qos::*` module agrees with them.

Four of the five agree and are asserted. `SYSTEM_DEFAULT` is asserted at its
CURRENT divergent values with a pointer to this issue, so the known gap is
recorded and any FURTHER drift still fails. It is a pinned bug, not a passing
test.

## Direction

* Decide the depth, update one side, delete the duplicate definition so the
  façade aliases rather than restates (the other four already could).
* Separately: decide whether `SYSTEM_DEFAULT` should carry upstream's
  sentinel meaning, which is an RFC-0036 divergence-row question and reaches
  the C ABI.
