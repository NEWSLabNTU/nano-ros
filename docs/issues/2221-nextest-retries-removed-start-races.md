---
id: 2221
title: "The 25 nextest `retries` overrides are gone; a red they used to mask is a start race to fix"
status: open
type: tech-debt
severity: medium
area: [testing, ci]
related: [0968, 0838]
found: 2026-11-03
---

## What

Maintainer ruling (2026-11-02): never add or raise a retry to make a test
pass. A test that fails on its first attempt and passes on a second has a
start race: a missing readiness edge, an ordering, or more concurrency than
its resources allow. The fix is that edge, or a narrower test-group
`max-threads`. Load is controlled through test parallelism, not re-rolls.

`.config/nextest.toml` carried 25 overrides that set `retries`, all removed
here. Gate `check-nextest-no-retries` (fast line) refuses any `retries` key
from now on, `retries = 0` included.

The justifications removed with them:
- the host-DDS "re-roll the PID → domain collision": already moot, since the
  domain slot partition (issue 0838) gives every live test its own block;
- "transient CPU-starvation timeout" on `binary(xrce)`;
- "deliberately left for a separate decision" on the three RTOS e2e groups
  (issue 0968's ~12 unreproduced tier-2 failures).

## What follows

Lanes that leaned on a retry will now show their reds. Each one is a start
race, and its fix belongs in the test or its fixture: wait for the readiness
banner instead of a fixed sleep, start the router before the image, or lower
the test-group's `max-threads`. Do not restore a retry. Record each race
found and fixed below, so this issue closes when the lanes run clean.

Overrides that carried a retry (filter | test-group | retries):

| filter | test-group | retries |
| --- | --- | --- |
| `binary(emulator) or binary(large_msg)` | qemu-baremetal | 2 |
| `binary(freertos_run_plan_runtime)` | qemu-freertos-entry | 1 |
| `binary(freertos_qemu) or (binary(rtos_e2e) and test(Platform__Freertos))` | qemu-freertos | 2 |
| `binary(c_riscv_nuttx_e2e)` | — | 1 |
| `binary(param_store_carve)` | — | 1 |
| `binary(typed_carrier_e2e)` | — | 1 |
| `binary(rtos_e2e) and test(Platform__Nuttx)` | qemu-nuttx | 0 |
| `binary(threadx_riscv64_qemu) or (binary(rtos_e2e) and test(Platform__ThreadxRiscv64))` | qemu-threadx-riscv | 2 |
| `binary(rtos_e2e) and test(Platform__ThreadxLinux)` | threadx-linux | 2 |
| `binary(zephyr) and test(xrce)` | qemu-zephyr-xrce | 2 |
| `binary(zephyr) and test(dds)` | qemu-zephyr-dds | 2 |
| `binary(zephyr) and test(zenoh_cpp_service_e2e)` | qemu-zephyr-service-cpp | 2 |
| `binary(zephyr) and test(zenoh_cpp_action_e2e)` | qemu-zephyr-action-cpp | 2 |
| `binary(zephyr) and (test(zenoh_cpp_pubsub_e2e) or test(cpp_talker_to_native_listener) or test(native_talker_to_zephyr_cpp_listener))` | qemu-zephyr-pubsub-cpp | 2 |
| `binary(zephyr) and (test(zenoh_rust_service_e2e) or test(native_server_zephyr_client) or test(zephyr_server_native_client))` | qemu-zephyr-service-rust | 2 |
| `binary(zephyr) and test(zenoh_rust_action_e2e)` | qemu-zephyr-action-rust | 2 |
| `binary(zephyr) and (test(zenoh_rust_pubsub_e2e) or test(zephyr_to_native_e2e) or test(native_to_zephyr_e2e) or test(bidirectional_native_zephyr_e2e))` | qemu-zephyr-pubsub-rust | 2 |
| `binary(zephyr)` | qemu-zephyr | 2 |
| `binary(fvp_runtime_ws)` | zephyr-fvp | 1 |
| `binary(fvp_smoke)` | zephyr-fvp | 1 |
| `binary(xrce)` | xrce | 2 |
| `binary(xrce_ros2_interop)` | host-dds-ros2-interop | 2 |
| `binary(interop_e2e) and test(cyclone)` | host-dds-ros2-interop | 2 |
| `binary(=advertised_state_interop)` | host-dds-ros2-interop | 2 |
| `binary(sched_dims_applied_e2e)` | matrix-consumers-serial | 2 |

## Found and fixed

(none yet)
