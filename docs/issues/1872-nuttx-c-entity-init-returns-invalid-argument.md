---
id: 1872
title: "NuttX C images: `rclc_subscription_init_default` and the typed `…__service_init` return RCL_RET_INVALID_ARGUMENT (-3) since the phase-492 C API rename"
status: open
type: bug
area: [nuttx, api, testing]
severity: high
found: 2026-10-13
related: [phase-492, 1838, 1836]
---

## Measured

Nightly 38346448295, dispatched on `main` at `d134dc83f0`. Job `nuttx`
(115129333390), step `Test / e2e (nuttx)`: 7 of 9 passed, and these 2 failed:

```
FAIL test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C
FAIL test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C
```

Both images boot, open the session and create the node, then fail the first
entity they create (from the junit artifact `nightly-nuttx-junit`):

```
nros C Listener
Locator: tcp/10.0.2.2:8300
Support initialized
Node created: listener
[nros] examples/qemu-armv7a-nuttx/c/listener/src/main.c:132
  rclc_subscription_init_default( &app.subscription, &app.node,
    ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/chatter") -> -3
```

```
nros C Service Server (AddTwoInts)
Locator: tcp/10.0.2.2:8310
Support initialized
Node created: add_two_ints_server
[nros] examples/qemu-armv7a-nuttx/c/service-server/src/main.c:137
  example_interfaces__srv__AddTwoInts__service_init(&app.service, &app.node,
    "/add_two_ints") -> -3
```

`-3` is `RCL_RET_INVALID_ARGUMENT` (`packages/api/nros-c/src/error.rs:41`,
asserted in `include/nros/rcl_compat.h`). The test then times out at 90 s
waiting for its readiness line.

## Window

- Last green: nightly 38251102237 (`199f34afbd`, 2026-10-12). The `nuttx`
  e2e passed 9/9, both of these cells included.
- First red: nightly 38346448295 (`d134dc83f0`, 2026-10-13).
- The phase-492 C API rename landed in between:
  - `cac4a9f31b` (W3/W5, `rcl_`/`rclc_` names);
  - `3c9d5e3033` (W7/W8, rosidl-shaped C messages, type support through the
    descriptor);
  - `ca52a58a7d` (W4, one executor object);
  - `63fced34a5` / `cd8a82b35c` (W1.b, conformed signatures);
  - `98d5509388` (examples include the API at upstream's paths);
  - `fd63b92705`, `d713df3f2b`.

## What is narrowed, and what is not

- Same lane, same run, PASSING:
  - NuttX Rust (pubsub, service, action);
  - NuttX C++ (pubsub, service, action);
  - NuttX **C action**.

  So the session, the board network and the router are fine (not issue
  1836/1838), and the C action path initialises its entities.
- Only the C subscription and the C service server fail. Both take a
  type-support handle: `ROSIDL_GET_MSG_TYPE_SUPPORT(...)` in the first case,
  and the generated typed `…__service_init` resolving its own in the second.
  A NULL or unrecognised type-support or descriptor reaching the
  `INVALID_ARGUMENT` check is the obvious lead. **Not measured**: the
  failing check was not identified, and the same images were not run on
  another platform. Whether the native or FreeRTOS C pubsub cells pass in
  the same nightly says whether this is NuttX-specific (the `freertos` job
  was green).
- The fixture probe printed `fixture reported FRESH by a DEGRADED probe — it
  compared NOTHING` for both NuttX C binaries. The images were built in this
  run, so they are current, but the probe is not evidence of that.

## Repro

`just setup nuttx && just build nuttx && just test nuttx`, or one cell:
`cargo nextest run -p nros-tests --test rtos_e2e -E
'test(test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C)'`
after the NuttX fixtures are built.

## What would close it

The two NuttX C cells pass in a nightly, with the cause named. If the cause
is a type-support or descriptor path that only the NuttX C road takes, add a
check that would have caught it before the nightly.
