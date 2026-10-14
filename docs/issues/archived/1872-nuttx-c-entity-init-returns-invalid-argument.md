---
id: 1872
title: "NuttX C images: `rclc_subscription_init_default` and the typed `…__service_init` return RCL_RET_INVALID_ARGUMENT (-3) since the phase-492 C API rename"
status: resolved
type: bug
area: [nuttx, api, testing]
severity: high
found: 2026-10-13
resolved: 2026-10-14
related: [phase-492, 1838, 1836, 1879]
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

## Resolution (2026-10-14) — fixed by issue 1879

Another session found and fixed the same cause the same day as issue 1879
(`d7909df96a`). That fix makes NuttX C and C++ TUs build without PIC,
through `nuttx_platform_build::flat_kernel_code_model`. This issue was diagnosed
independently, and the two diagnoses agree:

- **Measured** in QEMU with `gdb-multiarch` on the listener image.
  `rclc_subscription_init_default` received a NULL type support (`r2 = 0x0`)
  while `std_msgs__msg__String__nros_type` is defined at `0x680f78`.
- `.got` starts at exactly `_edata` (`0x40200da0`), and the flat kernel's
  startup copies only `[_sdata, _edata)`. So the GOT slot that
  `-fPIC` reads `&<Msg>__nros_type` through is never initialised.
- Phase-492 made `ROSIDL_GET_MSG_TYPE_SUPPORT` take that address, which is
  why the window opens there.

**Verified with the non-PIC build** (the same flag change as 1879, built
here before 1879 landed):

- `rtos_e2e` NuttX C passes 3/3 (pubsub, service, action).
- The riscv board's C talker (`c_riscv_nuttx_e2e`) delivers to a native
  listener. The riscv FFI crate calls the same `run_nuttx()`, so 1879's fix
  covers it too. Issue 1879 does not record a riscv run.

**Not measured:** why the C action cell passed while it was built with PIC.
`ROSIDL_GET_ACTION_TYPE_SUPPORT` also takes `&…__nros_type`. It no longer
matters, since no NuttX C object is PIC any more.
