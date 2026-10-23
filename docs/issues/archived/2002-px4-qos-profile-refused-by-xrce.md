---
id: 2002
title: "`QOS_PROFILE_PX4` demanded automatic liveliness, which the XRCE backend it exists for refuses, so no PX4 companion could advertise a topic"
status: resolved
type: bug
area: [rmw, px4, testing]
severity: high
found: 2026-10-23
resolved: 2026-10-23
related: [1329, 0026, phase-511]
---

## Measured

`cargo nextest run -p nros-tests --test px4_xrce` on fixtures freshly rebuilt
with `just px4 build-fixtures`, 2026-10-23, against a local `MicroXRCEAgent`.
Both cases failed within 3 s. `px4-stub` died at its first `create_publisher`:

```
Failed to advertise /fmu/out/vehicle_odometry: Transport(IncompatibleQos(QoSPolicyMask(128)))
```

Mask bit 7 is `QoSPolicyMask::LIVELINESS_AUTOMATIC`. The companion case fails
the same way, before printing anything.

## Cause

Issue 1329 (`13b3724623`, 2026-09-11) gave each C backend a
`supported_qos_policies` slot. XRCE honours none of deadline, lifespan or
liveliness (`uxrQoS_t` has no field for them), so it now refuses a profile
that states one. Its note in `session.c` says the C and C++ default profiles
were moved to the `None` sentinel in the same change, so as not to refuse
every application over a policy nobody asked for.

`QOS_PROFILE_PX4` (`packages/core/nros-rmw/src/traits.rs`) kept
`liveliness_kind: Automatic`, on the reasoning that a non-upstream shorthand
is a deliberate choice. That holds for `BEST_EFFORT` and `RELIABLE`. It does
not hold for this profile, which exists for the PX4 companion over XRCE: it was
the one profile guaranteed to be refused by the one backend it is for.

It stayed invisible for six weeks because `tests/px4_xrce.rs` was in no lane.
It had no matrix cell, and no CI runner carries the PX4-Autopilot `.msg` tree
its fixtures are generated from. Phase-511 B3 gave it a cell
(`Tier::Gated`), and the first run found this.

## Fix

`QOS_PROFILE_PX4` states `liveliness_kind: None`, like every upstream-mirroring
profile. The traits.rs note that names the three shorthands is corrected to two,
with this issue as the counterexample.

## Still true

`tests/px4_xrce.rs` runs only where `just px4 setup` has provisioned the PX4
tree (`Tier::Gated`), so a regression here is still caught only by someone who
runs that lane. The fixture is cheap (three host cargo builds plus `nros
generate-px4-msgs`); what keeps it off CI is the PX4-Autopilot checkout.
