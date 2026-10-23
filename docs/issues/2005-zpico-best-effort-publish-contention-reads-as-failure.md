---
id: 2005
title: "A BEST_EFFORT zenoh-pico publish dropped on TX-lock contention still returns PublishFailed, indistinguishable from a dead session"
status: open
type: bug
area: [rmw-zenoh, qos]
severity: low
found: 2026-10-23
related: [2001, 1980]
---

## What

Issue 2001 made a RELIABLE publisher wait for the transport TX mutex
(`Z_CONGESTION_CONTROL_BLOCK`). A BEST_EFFORT publisher keeps zenoh-pico's
default DROP, so a put that finds another thread holding the mutex is still
discarded, and `z_publisher_put` still returns an error. `zpico_publish` maps it
to `ZPICO_ERR_PUBLISH`, and `handles.rs` maps every backend error to
`TransportError::PublishFailed`. A caller sees the same error for "another
thread was mid-write" as for "the session is gone".

Losing the sample is what BEST_EFFORT allows. Reporting it as a failure is the
defect.

## Why 2001 did not also fix this

The fix proposed in 2001 (option 2) was a distinct `Busy` error. zenoh-pico
gives the caller no way to recognise contention: `_z_transport_tx_send_n_msg`
returns whatever the platform's `_z_mutex_try_lock` returned, and that differs
per port (`zenoh-pico/src/system/*/system.c`, measured 2026-10-23):

| port | try-lock failure |
| --- | --- |
| unix, zephyr, espidf, emscripten | `_Z_CHECK_SYS_ERR(pthread_mutex_trylock)` |
| freertos | `_Z_ERR_GENERIC` |
| rpi_pico, mbed | `-1` |
| flipper, arduino/opencr | `-1` unconditionally |

Every one of these values is also a code for some other failure. zpico cannot
tell them apart, and neither can anything above it.

## Options

1. **Fork patch: a contention drop under DROP is not an error.** In
   `_z_transport_tx_send_n_msg`, after `_Z_INFO("Dropping zenoh message because
   of congestion control")`, return `_Z_RES_OK`. This matches zenoh's own DROP,
   where a congestion drop is silent and `put` succeeds. The fork already has
   this shape one function up: `_z_transport_tx_try_send_t_msg` returns OK when
   the TX mutex is busy. One line, but it lives on our zenoh-pico patch branch
   for good.
2. **Fork patch: a dedicated result code** (e.g. `_Z_ERR_TRANSPORT_TX_BUSY`),
   surfaced as `TransportError::Busy`. Every caller then has to handle a code
   that BEST_EFFORT semantics say it should not care about. The C and C++ error
   mappings move too.
3. **Leave it.** Today no in-tree publisher is BEST_EFFORT on a path where
   anyone checks the result. 1980's test was RELIABLE.

Option 1 is the recommendation: it is the zenoh behaviour, and it removes the
error rather than giving it a name.
