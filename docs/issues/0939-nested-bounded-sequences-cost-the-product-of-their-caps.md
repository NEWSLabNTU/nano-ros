---
id: 939
title: "A bound over nested bounded sequences is the PRODUCT of the caps, so a uniform cap does not terminate"
status: open
area: codegen, memory
severity: medium
found: 2026-08-31
related: [0896, 0900, phase-403]
---

# One cap per level multiplies, and nothing says so

## What was measured

`nros_serdes::size::size_bound` walks a bounded sequence element by element, so
a type nesting bounded sequences costs the PRODUCT of the caps rather than a sum.
`visualization_msgs` nests five deep:

```
InteractiveMarkerInit -> markers -> controls -> markers -> points
```

A uniform cap of 128 at each level does not terminate in any useful sense: the
derived bound is 128^5 before the leaf is even counted.

## Why it is newly reachable

Pre-existing, but not previously reachable from configuration. Before phase-403
wired `CapacityResolver` into the derived bound, a config `cap` selected the
STORAGE container and never produced a `BoundedSequence`, so no amount of config
could build a deeply nested bounded chain. It can now, which is a good change --
it is what lets 121 of 126 stock Humble types carry a bound -- and it makes this
reachable by an ordinary user writing ordinary caps.

## Why it is not simply "cap smaller"

The multiplication is correct arithmetic for a worst case: five levels of 128
really can carry that many elements. The problem is that the number is useless
for sizing a receive buffer, and the failure mode is a bound so large it is
indistinguishable from unbounded -- except that it does not trip the unbounded
BUILD ERROR, so it fails later and less clearly.

## What would resolve it

Options, none chosen:

1. **Cap the derived bound**, with a diagnostic naming the nesting chain that
   produced it. The user then caps deliberately rather than discovering a number
   with five factors in it.
2. **A per-type total budget** in the codegen config, so a type states the size
   it may reach and codegen reports which nesting level blew it.
3. **Refuse to derive past a nesting depth** and require an explicit total.

Whatever is chosen must NOT silently substitute a number: phase-380's rule
stands, and a bound nobody chose is exactly what this campaign keeps removing.

## Adjacent, from the same work

`string[]` cannot be capped at all -- a config key names a FIELD and an array
element is not one -- so five stock types keep no bound: `sensor_msgs/
JointState.name`, `sensor_msgs/MultiDOFJointState.joint_names`,
`trajectory_msgs/JointTrajectory.joint_names`,
`trajectory_msgs/MultiDOFJointTrajectory.joint_names`, and
`visualization_msgs/InteractiveMarkerUpdate.erases`. The emitter spells an
element string from a built-in 256 that nobody chose; no bound is claimed from
it, correctly. Whether the config gains an element key is open.
