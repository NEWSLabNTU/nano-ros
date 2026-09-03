---
id: 962
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

Options:

1. **Cap the derived bound**, with a diagnostic naming the nesting chain that
   produced it. The user then caps deliberately rather than discovering a number
   with five factors in it. NOT chosen -- capping the derived bound IS
   substituting a number nobody derived.
2. **A per-type total budget** in the codegen config, so a type states the size
   it may reach and codegen reports which nesting level blew it. **CHOSEN
   (owner, 2026-08-31), landed as `max_serialized`.**
3. **Refuse to derive past a nesting depth** and require an explicit total. NOT
   chosen -- a depth limit is a threshold nobody can pick, and a legitimately
   deep type that fits comfortably would be refused.
4. **Make the multiplication visible** -- export the factor chain beside the
   total, so the number is legible whichever way a user then fixes it.
   **CHOSEN (owner, 2026-08-31), landed in the W6 inventory.**

Whatever is chosen must NOT silently substitute a number: phase-380's rule
stands, and a bound nobody chose is exactly what this campaign keeps removing.
Both landed options hold to it -- the budget is a CEILING CHECKED AGAINST, never
a value substituted, and a derived total under budget is exported unchanged.

## Landed 2026-08-31 (phase-403 W7b) -- options 2 and 4

### The measurement, on the type that motivated the issue

`/opt/ros/humble`, `visualization_msgs`, under the uniform cap of 128 this issue
names (`[defaults] string = 128, sequence = { cap = 128, element_cap = 128 }`):

| type | derived RX | worst chain |
| --- | ---: | --- |
| `InteractiveMarkerUpdate` | 19,379,320,485 | `markers.controls.markers.points = 128 x 128 x 128 x 128` |
| `InteractiveMarkerInit` | 19,379,256,985 | same |
| `InteractiveMarker` | 151,400,445 | `controls.markers.points = 128 x 128 x 128` |
| `MarkerArray` | 1,182,217 | `markers.points = 128 x 128` |
| `Marker` | 9,237 | none |

19.4 GB from caps of 128. "Does not terminate in any useful sense" is confirmed
with a number: the arithmetic completes in microseconds and the answer is
useless, and none of it trips the unbounded build error.

### Option 4 -- the factor chain, in the inventory

`schema_value::sequence_chains` walks the SAME `&'static [Field]` the bound is
derived from -- not a second derivation -- and reports every NESTED chain of
repeated members, deepest path first, with one factor per level. Depth 1 is
excluded: an ordinary container costs what it says, and listing every
`int32[<=4]` would bury the chains that matter.

Fixed arrays are factors too, deliberately: `size_bound` iterates them
identically, so a `Pose[100]` of a type carrying a `BoundedSequence(128)` really
does cost 12800 elements, and a chain that listed only the sequence would explain
the wrong number. What differs is the REMEDY, which the diagnostic says in prose
rather than by dropping a factor.

Exported on all three W6 transports off one model:

* JSON -- `"sequence_chains": [{"path": .., "factors": [..], "elements": N}]`
* CMake -- `_CHAIN_PATHS` / `_CHAIN_FACTORS` / `_CHAIN_ELEMENTS`, three PARALLEL
  lists so a consumer `foreach`es them natively instead of parsing a delimiter
* `build.rs` -- the same JSON document, compacted, on the existing `links`
  channel

Omitted entirely for a type that nests nothing, which is almost all of them.

### Option 2 -- `max_serialized`, a per-type budget

```toml
[types."visualization_msgs/InteractiveMarkerInit"]
sequence = 8
max_serialized = 8192
```

`[types.*]` only: `sequence`/`string` at a level are per-field CAPACITIES that
compose down the chain, and a total does not, so a `[defaults]` or
`[packages.*]` budget is a parse error rather than a key that quietly means
something different at each level.

The diagnostic, produced verbatim by the case above:

```
visualization_msgs/InteractiveMarkerInit: derived serialized-size bound 682297
bytes (RX; TX 606101) exceeds the `max_serialized = 8192` budget stated for this
type in nros-codegen.toml.
  The total is a PRODUCT: `nros_serdes::size` walks a bounded sequence and a
  fixed array element by element, so nesting MULTIPLIES (issue 0962). Cap ONE
  level of the worst chain and the whole product divides:
    markers.controls.markers.colors = 8 x 8 x 8 x 8 = 4096 elements
    markers.controls.markers.mesh_file.data = 8 x 8 x 8 x 8 = 4096 elements
    markers.controls.markers.points = 8 x 8 x 8 x 8 = 4096 elements
    markers.controls.markers.texture.data = 8 x 8 x 8 x 8 = 4096 elements
    markers.controls.markers.uv_coordinates = 8 x 8 x 8 x 8 = 4096 elements
    markers.menu_entries = 8 x 8 = 64 elements
  A factor that is a FIXED ARRAY comes from the `.msg` and no cap can change it;
  a bounded-sequence factor is either a `.msg` bound or a `cap` in
  nros-codegen.toml.
```

Raised as a BUILD ERROR from the C header emitter (which derives the bound the
`#define` states) and from `BoundInventory::check_budgets`, which every driver
calls once per package after recording every type -- so one build names every
type that blew its budget rather than one per rebuild.

### A budget-free type is genuinely unaffected

Asserted, not assumed (`a_type_with_no_budget_is_untouched`): no check runs, no
`max_serialized_budget` key appears on any transport, and the derived bound is
what it was. A budget the type FITS is equally inert -- the DERIVED total is
still what is exported (`a_budget_the_type_fits_never_becomes_the_bound`), which
is the phase-380 rule this issue insisted on. The whole in-tree golden corpus is
byte-identical across this change.

### Still open -- and ANALYSED 2026-09-03: not implementable as stated

This does not FIX the multiplication -- `size_bound` still walks a bounded
sequence element by element, and a deep chain still costs the product. What
changed is that the number is legible and that a user can make it a build error
instead of a runtime surprise. The remaining proposal was a size rule that treats
a nested bounded sequence as a bound on TOTAL elements rather than per-level.

**That is unsound in the RX direction, and the reason is issue 0896's rule one
level up.** A `cap` bounds OUR STORAGE. A remote ROS publisher is bound by the
`.msg`, not by `nros-codegen.toml`, so if the `.msg` permits 128 elements at each
of four levels a conforming peer really can send 128^4 of them. An RX bound
computed as a total would UNDER-REPORT, and `size_bound`'s own comment names that
as the dangerous direction: "an under-reported bound sizes a buffer too small and
reintroduces the very drop this exists to stop".

The tempting counter -- "our storage is capped, so we would reject the oversized
sample anyway" -- does not rescue it. Rejection happens after PARSING, and
parsing happens after the bytes have been received into a buffer. With a
size-classed payload pool an oversized sample is dropped at the TRANSPORT, which
is the silent drop, not a clean rejection.

**And it does not help TX either.** Our generated storage caps each level
independently; nothing enforces a total across a chain. A total-based TX bound
would be under-reported for the same reason, just against our own encoder.

So the multiplication is not a defect in `size_bound`. It is the correct worst
case for both directions, given that capacities are declared per level. There are
exactly two ways to make the number smaller and both already exist:

* **cap one level of the worst chain**, which divides the whole product -- what
  the option-2 diagnostic already tells the user to do, naming the chains; or
* **declare a total and enforce it**, which would require a wire-level element
  budget that neither ROS 2 nor our storage has.

**Recommendation: resolve this issue.** Options 2 and 4 landed, options 1 and 3
were rejected with reasons, and the fifth thing anyone would reach for is
unsound. What remains is not work but a property: a per-level capacity model
multiplies, and this issue is now the place that says so.

Flagged rather than closed unilaterally -- options 1-4 carry an explicit owner
decision (2026-08-31) and this reverses the "still open" line the owner wrote.

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
