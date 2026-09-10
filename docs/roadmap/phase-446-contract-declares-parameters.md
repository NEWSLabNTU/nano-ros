# Phase 446 -- the contract declares each node's parameters

**Status (2026-09-11). Open. W1 and W2 landed upstream (ros-launch-manifest
v0.1.34, play_launch 155ed78b) and are pinned; W4 and W6 landed; W3, W5 and W7
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
| description capacity | today the string capacity (F2) | the descriptor's `description` is a `String<MAX_STRING_VALUE_LEN>`, so an all-scalar image has no room for any description |
| service request/reply buffer | derived: the largest message the declared names and types can produce (F3); until then the configured `NROS_PARAM_SERVICE_BUFFER_SIZE`, one pair per executor (W5) | follows from the declarations and the board capacities |

Size bounds deliberately do NOT go in the contract: the contract describes the
node, and a capacity describes the board it is built for.

**Every node or none.** The store holds every node's parameters, so a count
over the nodes that declared is a count over a subset of the image. The
derivation runs only when every node in the image has a `params:` entry; when
some do and some do not, it refuses and the store knobs keep their configured
values. That makes "declares no parameters" a statement a node must be able
to make: `params: {}` means zero declared parameters (the node still gets its
`use_sim_time` seed), and a missing `params:` means "not stated". The two must
stay distinct all the way to the model (F1).

**Undeclared launch values.** A launch value for a parameter the node's
contract does not declare is an ERROR when it is addressed to the node by
name (an inline `<param>`, a parameter-file key equal to the node's name, a
launch-argument override), and a WARNING when it arrives through a wildcard
key such as `/**`. rclcpp ignores undeclared overrides, and a shared file
(Autoware's `vehicle_info.param.yaml`) is loaded into many nodes that each use
a subset; a typo in a node's own section is still caught. A value of the wrong
type is an error wherever it came from.

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

**Landed** in ros-launch-manifest v0.1.34 (`1a50d32`). Gap found after: an
explicitly empty `params: {}` parses the same as no `params:` (F1).

### W2 -- play_launch: the declared parameters reach the resolved model

The resolver carries each node's contract `params:` into the SystemModel next
to the node's other contract facts, keyed by the node's fully qualified name,
and validates the launch's parameter values for that node against the declared
types, by the rule in the design section: a mistyped value, or an undeclared
one addressed to the node by name, is an error naming the node, the parameter
and the launch file; an undeclared value that arrived through a wildcard key
is a warning. `use_sim_time`, `start_type_description_service` and
`qos_overrides.*` are always accepted.

*Acceptance:* a model built from a contract with `params:` carries them per
node; a mistyped or undeclared launch parameter fails resolution with a
message that names it.

**Landed** in play_launch `155ed78b`, with the wildcard warning in `f5264bf0`.
The messages name the parameter-file key and the launch file that loaded it,
not the file's path: the model keeps parameter files as contents only, and
naming the path needs a field in ros-launch-manifest's model.

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

**Landed** in #868: one pair per executor, allocated with the first node's
services; six servers per node and five lifecycle servers per executor
counted into the queryable pool; a dropped request logged once per service
and failure kind. Host `size_of`, mock backend, 4,096 B buffers: a four-node
image's service state went from 255,968 B to 68,096 B. The pair is still the
configured size -- deriving it is F3 -- and the board measurement is W7's.

### W6 -- the code is checked against the contract at boot

A generated per-node table of declared parameter names and types, consulted
by `declare_parameter`: a name the contract does not declare, or a type that
differs, refuses the boot and names the node, the parameter and the contract
file. This is the declared-QoS boot check (`check_declared_depth`) applied to
parameters, and it is what makes the gap between what the contract says and
what the code declares loud instead of a mis-sized store.

*Acceptance:* a node that declares a parameter missing from its contract, or
with a different type, refuses boot with that message; a matching node boots.

*Landed.* One derivation, `nros_orchestration_ir::declared_params`, feeds
both languages. C++: `nros ws entity-inventory --output-params-header`
renders `<nros/nros_declared_params_generated.h>` beside the declared-QoS
table, and `Node::declare_parameter` refuses through `set_error`
(`DECLARED_PARAM_MISMATCH`, -446). Rust: `nros::main!` hands the table to
the executor (`apply_declared_params`) before any node registers, and the
node runtime's parameter arm refuses registration. The check covers what the
APPLICATION declares, not the launch seed, whose value type is still
inferred from text (W3) and which the resolver already held to the contract.
Exempt, as play_launch exempts them: `use_sim_time`,
`start_type_description_service`, `qos_overrides.*`. Proven by the lookups'
`static_assert`s against a rendered fixture (`declared_params.cpp`) and by
unit tests of the Rust check; no image has booted a mismatching node yet
(W7).

### W7 -- the downstream end to end

Autoware Safety Island declares its 21 parameters in its contract, loads each
package's parameter file in its launch, and builds and boots on native,
native_sim and MR-CANHUBK344 with parameter services on. The demo passes on
the native and Zephyr islands.

*Acceptance:* the demo's VERDICT PASS on both islands; the board image's heap
use with parameter services measured and recorded.

## Follow-ups

### F1 -- `params: {}` survives to the model

A node that declares no parameters cannot say so today, and the every-node
rule then refuses the derivation for the whole image. The cause is the TYPE,
which every layer after it follows; it is not a YAML problem (yaml-rust2
yields an empty mapping for `{}`):

- ros-launch-manifest: `NodeDecl.params` is a `BTreeMap` (`types/src/types.rs`),
  so it has no third state. `parse_params` (`types/src/parse.rs`) returns the
  empty map both for a missing key (`Yaml::BadValue => return Ok(out)`) and
  for `{}`. Checked with a scratch program against v0.1.34: a node with no
  `params:` and a node with `params: {}` both parse to an empty map.
  `skip_serializing_if = "BTreeMap::is_empty"` drops it again on output.
- play_launch: `model_builder.rs` skips a node whose map is empty
  (`if node.params.is_empty() { continue; }`), so no `node_params` entry.
- nano-ros: `ParamDeclarations::from_model` (`entity_inventory.rs`) sees a
  node with no entry and refuses. It needs no change once the entry arrives:
  an entry with no names counts as declared, with the `use_sim_time` seed.

Fix: `params: Option<BTreeMap<String, ParamDecl>>` (`None` for no key,
`Some(empty)` for `{}`), serialized only when `Some`; the model builder
inserts `Some(empty)` as `/<fqn>: {}`, which `contracts.node_params` already
serializes because only the outer map skips when empty. The launch check then
treats every launch value for that node as undeclared, by the design rule.

*Acceptance:* a contract with `params: {}` on one node and names on the rest
resolves to a `node_params` entry for all of them, and the image derives its
store; dropping the `{}` refuses again, naming the node.

### F2 -- descriptions get their own capacity, and an overflow is not silent

`ParameterDescriptor.description` is a `String<MAX_STRING_VALUE_LEN>`
(`nros-params/src/types.rs`), so the description shares the capacity sized
for string VALUES. W4 derives that capacity to 0 for an all-scalar image, and
`with_description` then stores nothing: it clears the field and discards the
`push_str` error (`let _ =`), so a description one byte too long is dropped
whole rather than truncated, with no log. `ros2 param describe` then answers
an empty description.

Fix shape:

- a knob of its own, `NROS_MAX_PARAM_DESCRIPTION_LEN`, on the same ladder as
  the other per-slot knobs. A description is code-supplied text the contract
  does not declare, so its capacity is a board fact; 0 is a legitimate board
  choice meaning "no descriptions", stated rather than inherited;
- an overflow truncates at a character boundary and logs once per parameter,
  naming the knob, instead of emptying the field in silence;
- the same for any other `let _ = push_str` on a parameter's metadata.

Declaring description TEXT in the contract (it could then live in flash
instead of every slot) stays under "Not in this phase".

*Acceptance:* a scalar-only image with the description knob at 64 answers
`describe` with the code's text; a longer description is truncated with one
log line naming the knob.

**Landed** (F2 PR, on top of W4 #872). `NROS_MAX_PARAM_DESCRIPTION_LEN`, crate
default 256, on the per-slot ladder (env > Kconfig / `[knobs.params]
max_param_description_len` > default) and never derived; Zephyr gains
`CONFIG_NROS_MAX_PARAM_DESCRIPTION_LEN` (`range 0 256`), forwarded to cargo.
256 is the old effective capacity, so an image that states nothing keeps every
description it had, and it is the most the describe reply's
`rcl_interfaces` string carries; nros-node refuses a larger value at compile
time. Every description write (`with_description`, the typed builder, the
legacy builder) goes through one `fit_description`, which cuts at a UTF-8
boundary and records the cut on the descriptor. The typed builder used to
REFUSE the whole declaration with `StringConversion` instead. nros-params has
no logger, so the executor drains the record on each spin
(`ParameterServer::take_truncated_descriptions`, one flag test while nothing
is pending) and logs one warning per parameter naming the knob.

The C and C++ APIs have no description road at all (`params_shim.rs` and
`parameter.h` declare values only, and `ComponentNode` shares that facade), so
they reach neither the field nor the report. That makes the knob pure cost
for a C++ image. Measured `size_of`, at the downstream's derived limits (25
slots, names 35, string / array / byte-array 0):

| description capacity | slot | `ParameterStorage<25>` |
| --- | --- | --- |
| before F2 (shared with strings, derived 0) | 168 B | 4,200 B |
| 0 | 168 B | 4,200 B |
| 64 | 232 B | 5,800 B |
| 256 (default) | 424 B | 10,600 B |

So the downstream's C++ board should state `max_param_description_len = 0`.

### F3 -- the parameter-service buffer is derived from the declarations

W5 shares one request/reply pair per executor at the configured
`NROS_PARAM_SERVICE_BUFFER_SIZE` (4,096 B default). Its size should follow
from what can actually cross it:

- the largest REPLY over one executor's declared parameters, at the board's
  capacities: `describe_parameters` for every declared name (per descriptor:
  name, type, description, the additional-constraints string, flags, one
  range), `get_parameters` for every declared value, and `list_parameters`
  (every name plus its prefixes);
- the largest REQUEST a well-formed client can send for those parameters
  (`set_parameters_atomically` of every declared value). A request for names
  the image does not declare can be larger; it is refused and logged by the
  W5 path rather than sized for;
- CDR framing and alignment included, computed at build time by the entity
  inventory into `NROS_PARAM_SERVICE_BUFFER_SIZE` on the derivable ladder, and
  applied through `param_service_buffer_bytes()`
  (`nros-node/src/parameter_services.rs`), the one place W5 left for it;
- the configured constant stays the fallback when the declarations are absent
  or refused.

*Acceptance:* the downstream's image derives its buffer size and records it;
a unit test serializes the worst describe reply for a declared set and
asserts it fits the derived size and is within a small margin of it.

## Order

W1 -> W2 -> W4 and W6; W3 and W5 do not depend on the schema and can start at
once. W7 needs all of them. F1 lands in ros-launch-manifest and play_launch
and needs no nano-ros change beyond a pin; F2 is independent; F3 needs W4.

## Not in this phase

- Starting the parameter services on Cyclone (issue 1268) and presenting every
  node to the ROS graph on every RMW (issue 1269). Both are needed for a ROS 2
  tool to reach the parameters on Cyclone, and neither changes how anything is
  sized.
- A per-type parameter pool. Zero capacity for absent types already takes the
  scalar case to its floor.
- Descriptions and ranges in the contract. The map form leaves room for them.
