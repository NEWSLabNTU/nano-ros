---
id: 2403
title: "On the emulated Pixhawk 6X (PX4 NuttX), a nano-ros uORB subscription registers and its broker callback fires, but the executor never takes a sample: `vehicle_status` is published by PX4's commander and the demo counts 0"
status: open
type: bug
severity: high
area: [px4, nuttx, rmw, executor]
related: [phase-547, 2218, 2225, 1940, 2398]
found: 2026-11-10
---

**Home: [phase-547](../roadmap/phase-547-px4-pixhawk-emulator-spike.md)**, W3
(the uORB round trip). Found once [issue 2398](archived/2398-px4-thread-stacks-below-executor-floor.md)'s
stack remedy removed the `mm_free` fault that had stopped the module before.

## What

The firmware example's `nros_uorb_demo` (examples/px4/cpp/firmware) publishes
`debug_key_value` and subscribes `vehicle_status`. On the emulated 6X
(Renode 1.17.0, the phase-547 scripted repro, image `px4_fmu-v6x_wqstk`:
`STACK_MAIN 16384`, `CONFIG_WQ_RATE_CTRL_STACKSIZE=8192`):

- **Publish works.** PX4's own `listener debug_key_value` prints
  `key: "nros"`, `value: 7.00000`.
- **Subscribe does not.** PX4's commander publishes `vehicle_status`
  (`listener vehicle_status` prints `nav_state: 4`, `arming_state: 1`), yet the
  demo's stats line reads `status=0` at 10, 20 and 30 ticks (30 s of virtual
  time), and `recv vehicle_status` never prints.

## Measured (instrumented scratch build, not committed)

`printf`s were added to `packages/rmw/uorb/nros-rmw-uorb/src/subscriber.cpp`
in the scratch copy only:

| point | result |
|---|---|
| `subscription_create` | `vehicle_status handle=3 reg_rc=0`: subscribed, broker callback registered (`callback_active`) |
| `subscriber_ready_callback` | fires (3 of 3 logged calls), from commander's publish; `on_new` is set, to `nros_rmw_cffi::entity_wake::runtime_entity_wake` |
| `subscription_take` | **never called** (0 logged calls in 20 executor passes) |

`nros_uorb_demo status` reports the sched task's work item ran 20 times
(`runs=20`), so the executor passes happen (the 1 Hz publish timer drives
them). The subscription is created ready (`ready = true` at create), so even
the first pass should have taken. So the defect is between the runtime's
per-entity wake (`WakerSlot::wake`) and the executor's dispatch: a pass never
asks the uORB subscription for data. It is not the broker, the callback pool
or the session.

## Not yet known

- Whether PX4 SITL shows the same. The firmware example's README records a
  SITL run that received `vehicle_status`, but that predates phase-521 W8 (d)
  (issues 2218 and 2225 changed this wake path). A SITL run of the current
  tree answers whether this is NuttX-specific or a general regression.
- Whether the spike's scratch measures touch it. They replace `std::atomic`
  with `__atomic` builtins in `session.cpp` and `subscriber.cpp` (PX4's
  `-nostdinc++` has no `<atomic>`); the instrumented `ready_cb` shows those
  stores and loads behave.

## Reproduce

The phase-547 doc's "Reproducing the spike (scripted, 2026-11-10)" builds the
image; then

```bash
repro/box env SEND='listener vehicle_status;nros_uorb_demo status' \
  repro/21-renode-run.sh build-wqstk/px4_fmu-v6x_wqstk.bin repro/run-wqstk 'value=29' 420
```
