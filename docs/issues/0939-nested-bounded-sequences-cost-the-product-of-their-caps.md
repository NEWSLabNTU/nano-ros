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

## Adjacent, from the same work -- RESOLVED 2026-08-31 (phase-403 W7)

`string[]` could not be capped at all -- a config key named a FIELD and an array
element is not one -- so five stock types kept no bound: `sensor_msgs/
JointState.name`, `sensor_msgs/MultiDOFJointState.joint_names`,
`trajectory_msgs/JointTrajectory.joint_names`,
`trajectory_msgs/MultiDOFJointTrajectory.joint_names`, and
`visualization_msgs/InteractiveMarkerUpdate.erases`. The emitter spelled an
element string from a built-in 256 that nobody chose; no bound was claimed from
it, correctly.

The config now has the element key: `element_cap`, beside `cap` in the same
entry, mirroring the two dimensions a `.msg` already spells as
`string<=10[<=5]`. All five are bounded; the 12-package corpus goes 121 -> 126
of 126.

## What W7 measured about THIS issue

**The element key does not deepen the product, and cannot.** A string is a LEAF:
`element_cap` turns `String` into `BoundedString`, which has no elements of its
own, so it adds a LINEAR factor to one level (`cap * (4 + element_cap + 1)`,
each element padded to 4) and never a level to a bounded-sequence chain. The
depth the product is taken over stays a property of the `.msg`. Pinned by
`schema_value::tests::an_element_cap_cannot_deepen_a_bounded_sequence_chain`.

**But the product is now VISIBLE in a stock type, and it is not small.** Under
the W7 measurement config, `visualization_msgs/InteractiveMarkerUpdate` bounds
at **34,158,429 bytes (TX) / 34,234,821 (RX)** -- from a config whose largest
single cap is 65536 and whose sequence caps are 8 to 64. Its `markers` chain is
the `InteractiveMarker -> controls -> markers -> points` nesting named above.
That is a bound in the sense that the arithmetic terminates, and useless for
sizing a receive buffer, which is exactly what this issue says. It is now a
number a user can PRODUCE rather than a hypothetical, so option 1 (cap the
derived bound, with a diagnostic naming the chain) has a concrete case to be
designed against.

**No diagnostic was added, and here is why not.** Naming the nesting chain is
cheap to COMPUTE -- one walk of the built `&'static [Field]` -- and has nowhere
to go. `bound_message` returns a `TypeBound` with three variants and no advisory
channel; `BoundState` is the shared classification the inventory and the C
header both read, so a fourth state changes the exported schema and every
consumer of it. Inventing that channel is a design decision this issue already
lists three options for, and picking one inside an unrelated change is how a
substituted number gets shipped. The observation is recorded here instead.
