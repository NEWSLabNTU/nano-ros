# Phase 446 -- the contract declares each node's parameters

**Status (2026-09-11). Open. W1 and W2 landed upstream (ros-launch-manifest
v0.1.34, play_launch 155ed78b) and are pinned; W4 landed; W3, W5, W6 and W7
open.** The launch contract gains a
per-node `params:` section that states each parameter's NAME and TYPE. Sizing
the parameter store and the parameter services moves from built-in worst cases
to what the contract declares, with board configuration supplying the one
thing a contract cannot: capacities for strings and arrays.

**Related issues:** 1268 (Cyclone parameter services never start), 1269 (the
graph node set differs across RMWs), 1270 (48 KiB of service buffers per
node), 1271 (silent reply overflow), 1272 (launch parameters land on node 0),
1255 (per-type slot pricing for messages, the same idea for the arena).

## Why this exists

Measured on a downstream (Autoware Safety Island: four C++ component nodes on
one executor, 21 scalar parameters):

- The parameter store is sized for the worst value any parameter could hold.
  Every slot reserves an array of 32 strings of 256 characters: 8,920 B per
  slot, 285,440 B for the default 32 slots. Sized for scalars (strings, arrays
  and byte arrays at 0, names at 35) the same store is 168 B per slot, 4,200 B
  for the 25 slots the image needs. Measured with `size_of` against
  `nros-params` built at each setting.
- On Zephyr that store alone exhausts the default 64 KiB nros heap at boot
  (`HEAP EXHAUSTED: request 285440 bytes`), and only `NROS_MAX_PARAMETERS` can
  be set: the four per-slot limits have no Kconfig row and no board rung that
  reaches cargo.
- The parameter services add 6 x (4 KiB + 4 KiB) of buffers per node (issue
  1270), 196 KiB for four nodes: more than the downstream board's whole
  94 KiB heap before the store is counted.

Extracting parameter sizes from source code would need real static analysis
and break on the first parameter declared in a loop. The launch toolchain
already reads the contract and the launch's parameter files per node, so the
contract is the place to state what each node declares, the way it already
states `pub`, `sub`, `srv` and `cli`.

## The design, and why the split

| number | source | why there |
| --- | --- | --- |
| parameter names and types, per node | the contract `params:` | platform-agnostic, like the topics a node publishes |
| values | parameter files and launch overrides | ROS 2 practice; a deployment changes them without touching the contract |
| slot count | derived: sum of contract params + one `use_sim_time` per node | the phase-430 W2 seed is per node |
| max name length | derived: the longest contract name | platform-agnostic |
| string / array / byte-array capacity | 0 when no node declares that type; otherwise the BOARD states it, and the build refuses when it does not | capacity is a board fact (an MCU and a PC want different ones), and a bound must exist before a buffer can be sized from it, the rule messages already follow |
| service reply buffer | derived: the largest reply the declared names and types can produce (issue 1270) | follows from the declarations |

Size bounds deliberately do NOT go in the contract: the contract describes the
node, and a capacity describes the board it is built for.

The contract entry, in the map style of `sub:` / `pub:` so later fields
(`read_only`, a description) are additive:

```yaml
nodes:
  mrm_handler:
    params:
      update_rate: { type: integer }
      timeout_operation_mode_availability: { type: double }
      use_emergency_holding: { type: bool }
      turning_hazard_on.emergency: { type: bool }
```

Types are the ROS 2 parameter types: `bool`, `integer`, `double`, `string`,
`byte_array`, `bool_array`, `integer_array`, `double_array`, `string_array`.

For an image whose parameters are all scalars, the contract alone sizes the
store exactly and no board capacity is ever consulted. The storage type does
not have to change for that: every slot is as large as the largest variant the
limits allow, so zero capacity for the absent types is what takes a slot from
8,920 B to 168 B. A per-type pool is a later optimisation, not a prerequisite.

## Work items

### W1 -- ros-launch-manifest: `NodeDecl` gains `params`

`params: BTreeMap<String, ParamDecl>` on the per-node declaration, with
`ParamDecl { type: ParamType }` and room for optional fields. `ParamType` is the
nine ROS 2 types above; an unknown type is a parse error, never a skip. Round
trips through serde, and a contract without `params:` parses exactly as before.

*Acceptance:* schema tests for every type, an unknown type, a missing type,
and an absent section; a release tag the other repos pin.

### W2 -- play_launch: the declared parameters reach the resolved model

The resolver carries each node's contract `params:` into the SystemModel next
to the node's other contract facts, keyed by the node's fully qualified name,
and validates the launch's parameter values for that node against the declared
types (a launch value for an undeclared name, or one that does not parse as
the declared type, is an error naming the node, the parameter and the file).

*Acceptance:* a model built from a contract with `params:` carries them per
node; a mistyped or undeclared launch parameter fails resolution with a
message that names it.

### W3 -- launch parameters are seeded on their own node (issue 1272)

The C++ entry emits the node with each parameter, and the shim declares on
that node (`declare_parameter_on`); the Rust `nros::main!` path stops
flattening every node's parameters into one list. With W2, the value's type
comes from the declaration instead of being inferred from the text.

*Acceptance:* a two-node entry that sets the same parameter name on both nodes
boots, and each node reads its own value.

### W4 -- the store is sized from the declarations

The entity inventory derives `NROS_MAX_PARAMETERS` and
`NROS_MAX_PARAM_NAME_LEN` from the model, and sets the string, array and
byte-array capacities to 0 when no node declares that type. When some node
does, the capacity comes from the board (the existing `[knobs.params]` rung,
and on Zephyr new Kconfig rows for the four per-slot limits, forwarded to
cargo the way `NROS_MAX_PARAMETERS` is), and the build REFUSES, naming the
parameter and the knob, when the board states none. A bringup with no
contract `params:` keeps today's defaults: absence is not zero.

*Acceptance:* the downstream's four-node image derives 25 slots and a 4,200 B
store (measured on the linked image), and a string parameter with no board
capacity fails the build naming both.

*Landed.* `nros_cli_core::entity_inventory::ParamDeclarations` derives the
numbers; nros-params' build script takes a capacity from a stated rung or
refuses. Measured with `size_of` against nros-params built at the derived
limits (25 slots, names 35, capacities 0): 4,200 B. The downstream image
itself is W7's to measure. Two things W4 did not settle: a node cannot yet
declare "no parameters" (the resolver drops an empty `params:`), so an image
with a parameterless node refuses and keeps the defaults; and at a derived 0
string capacity no parameter description fits, which
`ParameterDescriptor::with_description` still drops silently.

### W5 -- the parameter services are sized, shared and counted (issue 1270)

One request/reply buffer pair serves every parameter service of an executor,
sized from the largest reply the declared parameters can produce, with the
current constant as the fallback when no declarations exist. The entity
inventory counts six service servers per node whenever `param_services` is
declared, so the RMW pools are sized for them.

*Acceptance:* the downstream's four-node image fits its board heap with
parameter services on, measured, and a reply past the derived size is
refused with a log line naming the knob (issue 1271).

### W6 -- the code is checked against the contract at boot

A generated per-node table of declared parameter names and types, consulted
by `declare_parameter`: a name the contract does not declare, or a type that
differs, refuses the boot and names the node, the parameter and the contract
file. This is the declared-QoS boot check (`check_declared_depth`) applied to
parameters, and it is what makes the gap between what the contract says and
what the code declares loud instead of a mis-sized store.

*Acceptance:* a node that declares a parameter missing from its contract, or
with a different type, refuses boot with that message; a matching node boots.

### W7 -- the downstream end to end

Autoware Safety Island declares its 21 parameters in its contract, loads each
package's parameter file in its launch, and builds and boots on native,
native_sim and MR-CANHUBK344 with parameter services on. The demo passes on
the native and Zephyr islands.

*Acceptance:* the demo's VERDICT PASS on both islands; the board image's heap
use with parameter services measured and recorded.

## Order

W1 -> W2 -> W4 and W6; W3 and W5 do not depend on the schema and can start at
once. W7 needs all of them.

## Not in this phase

- Starting the parameter services on Cyclone (issue 1268) and presenting every
  node to the ROS graph on every RMW (issue 1269). Both are needed for a ROS 2
  tool to reach the parameters on Cyclone, and neither changes how anything is
  sized.
- A per-type parameter pool. Zero capacity for absent types already takes the
  scalar case to its floor.
- Descriptions and ranges in the contract. The map form leaves room for them.
