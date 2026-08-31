---
id: 964
title: "The C++ header states an ESTIMATED size for every type, including types that have no bound"
status: open
area: codegen
severity: medium
related: [0896, 0939, 0940, phase-403, phase-380]
---

# One type, two numbers, and the wrong one is the one in the header

## What was measured

`rosidl-codegen`'s C++ pack computes `SERIALIZED_SIZE_MAX` with
`compute_serialized_size_max`, which ESTIMATES: a flat 512 per nested message, a
flat default capacity per string, and it ALWAYS returns a value. It therefore
cannot report "unbounded".

Over 120 types in 12 stock ROS Humble packages:

* **81 of 120 have no derived bound, and the C++ header states a size for every
  one of them.**
* Of the 39 that are bounded, the estimate matched the derived bound **zero
  times**: 38 over, 1 under. `geometry_msgs/Twist` reads 1028 against a derived
  64.

On the island entry the same divergence, per type:

| type | C++ header | derived |
| --- | ---: | ---: |
| `autoware_control_msgs/Control` | 2052 | 114 |
| `nav_msgs/Odometry` | 1804 | unbounded, until a cap |
| `autoware_vehicle_msgs/SteeringReport` | 527 | 24 |
| `autoware_adapi_v1_msgs/OperationModeState` | 572 | 27 |

## Why it matters

phase-403 W6 exports the DERIVED bound. The C++ header keeps emitting the
ESTIMATE. The same type now carries two numbers, and the estimate is the one a
user reads, since it is the constant the header advertises and the one
`{Msg}_RX_MAX_SERIALIZED_SIZE` names.

It also violates phase-380's rule directly: a number nobody chose, substituted
where the honest answer is "no bound exists". That rule is why an unbounded type
is a build error at all, and this path quietly opts out of it.

Real cost, not hypothetical: the island's sizing was planned against the
estimate, so the receive buffer, arena and payload classes were all budgeted for
`Control` at 2052 bytes when it serializes to 114.

## Options

1. **Delete the estimator** and emit the derived bound, poisoning the constant
   for an unbounded type exactly as the C pack already does
   (`unbounded_token` + `unbounded_reason`). Behaviour change: a type that is
   unbounded stops having a size constant, which is the point.
2. **Keep both, renamed.** The estimate becomes something honestly named
   (`..._ESTIMATED_SIZE`) and the derived bound takes the load-bearing name.
   Cheaper, but leaves a number nobody chose in the header.
3. **Emit the derived bound only where one exists** and nothing otherwise,
   which is (1) without the poison token.

(1) matches what the C pack does today and what phase-380 requires. It is a
behaviour change with a blast radius across every C++ consumer, which is why it
is filed rather than done.
