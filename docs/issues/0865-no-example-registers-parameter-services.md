---
id: 865
title: "no example registers parameter services, so `ros2 param list` returns
  nothing against every nano-ros node — the capability exists and is never shown"
status: open
type: bug
area: examples, docs
related: [issue-0864]
---

## Symptom

Against the CANHUBK344 action-server image, with a `demo_nodes_cpp` talker on
the same router as a control:

```
$ ros2 param list /fibonacci_action_server
                                     <- nothing

$ ros2 param list /talker
  qos_overrides./parameter_events.publisher.depth
  qos_overrides./parameter_events.publisher.durability
  ...
```

`ros2 service list` tells the same story from the other side:

```
/talker/describe_parameters
/talker/get_parameter_types
/talker/get_parameters
/talker/list_parameters
/talker/set_parameters
/talker/set_parameters_atomically
```

Six for the host node, none for the board's.

## Cause

`Executor::register_parameter_services()` (`executor/spin.rs:6388`, exposed to C
as `nros_executor_register_parameter_services`) is **opt-in**, and:

```
$ grep -rl register_parameter_services examples/
                                     <- no match
```

Not one example in the tree calls it. The capability is fully implemented —
`parameter_services.rs` serves all six — and nothing demonstrates it.

## Why opt-in is defensible and the current state is not

Opt-in is the right default: the six servers cost RAM on a part where that is
the binding constraint, and an image with no parameters should not pay for
them. That is not the complaint.

The complaint is that every nano-ros node therefore looks, to standard ROS 2
tooling, like a node whose parameter interface is broken rather than one that
declined to have it. `ros2 param list` returning empty is indistinguishable
from a node that is failing to answer, which is exactly the kind of ambiguity
that cost issue 0852 six wrong hypotheses.

## What is NOT wrong here

Worth recording, because it looks like a defect and is not.

The board's action services do not appear in a bare `ros2 service list`:

```
$ ros2 service list --include-hidden-services
/fibonacci/_action/cancel_goal
/fibonacci/_action/get_result
/fibonacci/_action/send_goal
```

`ros2 service list` hides `_action/` services by default. All three are
registered, correctly named, correctly type-mangled, and they work — goals
complete with the right sequence. Same for `ros2 node info` showing an empty
"Service Servers" block while "Action Servers" lists `/fibonacci`.

## Fix direction

1. Call `register_parameter_services()` in at least one Zephyr example, and
   declare a parameter in it, so the path is exercised on hardware rather than
   only in host tests.
2. Say so where a reader will hit it: the examples README and the parameter
   docs should state that `ros2 param` needs this call, and that omitting it is
   a footprint choice rather than a missing feature.
3. Consider a boot-time log line when a node comes up without parameter
   services — one line, once, so the empty `ros2 param list` explains itself.
