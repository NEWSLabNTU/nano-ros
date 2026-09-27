---
id: 1506
title: "An action row's `wire_bound_bytes` was the SendGoal request alone, but the
  three queryables share one ring and `action_msgs/srv/CancelGoal_Request` is
  bigger — 44 B against 36 on the in-tree Fibonacci, so the cancel queryable's
  slot was 8 B short"
status: resolved
type: bug
area: [cli, core, build]
severity: high
found: 2026-09-27
related: [1352, 1393, 1378, 0196]
---

## What was wrong

`nros-rmw-zenoh` gives an action server's three queryables — `send_goal`,
`cancel_goal`, `get_result` — slots from **one** ring: `ACTION_INBOX`, selected
by the `/_action/` infix in `shim/service.rs`'s `inbox_for`, with
`ACTION_INBOX_QUERYABLES = <action servers> × 3`. So the slot size
`NROS_ACTION_INBOX_BYTES` has to hold the largest of the three requests.

Two producers derived it from **one** of the three:

* the leaf road — `sizing_descriptor::wire_type_of` mapped an
  `EndpointKind::ActionServer` row to `rosidl_codegen::action_request_type`,
  i.e. `<A>_SendGoal_Request`, and the row's `wire_bound_bytes` was that alone;
* the cmake road — `EntityInventory::action_request_types()` published the same
  single spelling into `NROS_ENTITY_ACTION_REQUEST_TYPES`, which
  `cmake/NanoRosEntityInventory.cmake` maxes over to get
  `NROS_DERIVED_ACTION_INBOX_BYTES`.

Both carried the same reason at the code: *"only SendGoal carries the user's own
goal, so it is the one that can exceed the other two."* That is false whenever
the goal struct is smaller than a `GoalInfo`, which is the ordinary case.

## Measured

`examples/native/rust/action-server`
(`example_interfaces/action/Fibonacci`, `int32 order` goal), from the leaf's own
`generated/*/nros_message_bounds.json`:

| type | rx bound |
| --- | --- |
| `example_interfaces/action/Fibonacci_SendGoal_Request` | 36 |
| `action_msgs/srv/CancelGoal_Request` | **44** |
| `example_interfaces/action/Fibonacci_GetResult_Request` | 28 |

`CancelGoal_Request` is `{ GoalInfo goal_info }` = a UUID plus a `Time`, and it
outweighs a 4-byte goal. So the built image had
`ACTION_INBOX_BYTES = 36` against a 44-byte request: over the wire that lands as
`ServiceRequestSlot::overflow`, the payload is skipped, and `take_request` pops
it as `TransportError::MessageTooLarge` — **every cancel request on that image,
silently, with the ring reporting a size the build called derived.**

Nothing caught it because the number was derived correctly and delivered
faithfully; only the POPULATION it was a maximum over was wrong. That is the
same shape as issues 1015/1033 (a floor in the shared derivation defeating the
consumer that named the knob) and 0196's class generally.

## Fixed

phase-457 W1. `rosidl_codegen::action_received_types` names the three request
types, spelled once, and both producers take the maximum over the set:

* the descriptor's `received_types_of` maxes the three and **REFUSES the whole
  row** if any member is unpriced — a maximum over the members that answered is
  not a bound on the ones that did not, which is the rule
  `declared_service_request_bytes` already applies per family;
* `action_request_types()` publishes all three, and its consumer already maxes.

`ACTION_INBOX_BYTES` on the named image goes 36 → 44, and static RAM
172,698 → 172,794 B (**+96 B**). The fix COSTS bytes, because it closes an
under-size; the direction is the point.

One of the three belongs to `action_msgs`, not to the action's package, so a
consumer joining on this list has to reach a bound inventory covering
`action_msgs` too. `record_action` deliberately does not price it under the
action's name (it is another package's service, priced there by
`record_service`), so the join spans two inventories and refuses rather than
skips when one is missing. The fail-first reproduction for that is
`an_action_row_refuses_when_one_of_its_three_requests_is_unpriced`.

## The same defect in the `[types]` maxima, fixed in the same wave

`type_facts` joined the CycloneDDS schema shape on the endpoint's **interface**
name (`pkg/srv/Name`, `pkg/action/Name`). No such type is a message, codegen
prices none of them, so `max_fields` / `max_kinds` / `max_nested_depth` were
refused on every service and action image — with a reason telling the reader to
run `nros sync`, a remedy that cannot work.

They now join on the types the endpoint REGISTERS
(`sizing_descriptor::registered_types_of`): a service's two halves, an action's
eight members (the five envelopes plus `_Goal` / `_Result` / `_Feedback`, which
`RosAction` registers in their own right and which `record_action` now prices)
plus the three `action_msgs` protocol types
`RosAction::register_protocol_types` registers. Omitting those last three would
be this same under-size one level up: measured on the same leaf, they hold the
DEEPEST schemas an action image carries — `CancelGoal_Response` 11 kinds and
`GoalStatusArray` nested_depth 6, against 7 and 3 for the envelopes.

Emitted knob delta on `examples/native/rust/action-server`, built twice and
diffed:

```
+ NROS_CYCLONEDDS_MAX_FIELDS = "4"          (was: no row, consumer kept 64)
+ NROS_CYCLONEDDS_MAX_KINDS = "11"          (was: no row, consumer kept 256)
+ NROS_CYCLONEDDS_MAX_NESTED_DEPTH = "6"    (was: no row, consumer kept 8)
```

`dynamic_type.rs`'s own arithmetic for the descriptor builder's name arrays is
`MAX_FIELDS × 64 + MAX_KINDS × 64`: **20,480 B → 960 B** of stack frame, plus
`[NrosFieldDescriptor; MAX_FIELDS]` and `[NrosFieldKindDescriptor; MAX_KINDS]`
beside them. Stack, so `mem-report` does not see it.

## Not changed, and why

`[types] distinct_count` still counts declared interfaces, not registered
message types. The registered COUNT has its own producer —
`nros_orchestration_ir::cyclonedds_type_sizing`, which resolves
`NROS_CYCLONEDDS_MAX_TYPES` from the SystemModel (a srv is 2, an action 8 + 3) —
and a second answer here is exactly what the single-writer rule beside
`WrittenDescriptor::cyclonedds_env` refuses. The two sets differing inside one
section is recorded in a comment at `type_facts` rather than left to be
rediscovered.
