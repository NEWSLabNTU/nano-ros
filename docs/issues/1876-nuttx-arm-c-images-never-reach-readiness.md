---
id: 1876
title: "NuttX-ARM C images create their node and then fail `rclc_subscription_init_default` with `RCL_RET_INVALID_ARGUMENT` (-3), so four tier-2 cells never reach readiness"
status: open
type: bug
severity: high
area: [nuttx, c-api]
related: [phase-492, 1874, 1875]
found: 2026-10-13
---

## Measured

Tier 2 (`just ci matrix`, the phase-486 W8c tree, 2026-10-13), four real
failures, all on the NuttX-ARM C coordinate:

- `rtos_e2e::test_rtos_pubsub_e2e::…Nuttx…Lang__C`: readiness pattern
  `Waiting for messages` not observed;
- `rtos_e2e::test_rtos_service_e2e::…Nuttx…Lang__C`: `Waiting for service
  requests` not observed;
- `typed_carrier_e2e::nuttx_arm_typed_carrier_boots_and_publishes`: published
  nothing, empty console;
- `realtime_tiers_e2e::realtime_tiers`: the `nuttx-arm/c` row's low tier never
  reached 5 deliveries.

The listener's console (`qemu-system-arm -M virt -cpu cortex-a7`):

```
nros C Listener
Locator: tcp/10.0.2.2:8300
Domain ID: 0
Support initialized
Node created: listener
[nros] …/examples/qemu-armv7a-nuttx/c/listener/src/main.c:132
  rclc_subscription_init_default( &app.subscription, &app.node,
  ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/chatter") -> -3
```

## What -3 can be

`-3` is `RCL_RET_INVALID_ARGUMENT`. In `rclc_subscription_init_with_qos` it has
exactly two sources:

1. **a NULL among `subscription`, `node`, `type_info`, `topic_name`**, from
   `validate_not_null!`. `ROSIDL_GET_MSG_TYPE_SUPPORT` takes the address of a
   global, so `type_info` cannot be NULL;
2. **a zero-length topic copy into `subscription.topic_name`.**

A wrong state would return `-4` (`RCL_RET_NOT_INIT`) or `RCLC_RET_BAD_SEQUENCE`,
not `-3`.

On ARM, both sources fit one shape: the C side and the Rust side disagree on
the layout of a struct the call passes. That is the short-enums class from
issue 0238: a `repr(C)` enum is 1 byte on `armv7a-nuttx-eabihf` and 4 on
x86_64. Phase-492 renamed and reshaped exactly these structs (`rcl_node_t`,
`rcl_subscription_t`, the C message type support).

The same run also warned that both NuttX C images were reported FRESH by a
DEGRADED staleness probe and had been rewritten at 13:18, during the run. So
the image's age against the tree is itself unverified.

## Next

Rebuild tier 2 on current `main` and re-run these four cells SOLO before
reading anything further. Then compare `sizeof`/`offsetof` of the
phase-492 structs between the C headers and the Rust definitions on
`armv7a-nuttx-eabihf`.
