---
id: 2005
title: "A BEST_EFFORT zenoh-pico publish dropped on TX-lock contention still returns PublishFailed, indistinguishable from a dead session"
status: resolved
resolved: 2026-10-23
resolved_in: "zenoh-pico fork 57bbde9e"
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
gives the caller no way to recognise contention. Whatever the failed try-lock
returned, `_z_write` replaces it with `_Z_ERR_TRANSPORT_TX_FAILED`, the same
code a real send failure gets. And the try-lock's own value is not stable
either:

- nano-ros builds zenoh-pico as `ZENOH_GENERIC`, so `_z_mutex_try_lock` is
  `nros_platform_mutex_try_lock` (`zpico-sys/c/zpico/platform_aliases.c`). On
  POSIX that returns `1` on `EBUSY`, a positive value.
- zenoh-pico's own system ports differ again (`zenoh-pico/src/system/*/system.c`):
  `-1` on rpi_pico, mbed, flipper and arduino, `_Z_ERR_GENERIC` on FreeRTOS, and
  an errno mapping through `_Z_CHECK_SYS_ERR` on unix, zephyr and espidf.

**Where the contention comes from** (measured in the pinned fork, 2026-10-23).
Every data send takes the session's transport lock first (`_z_send_n_msg`,
issue 0899), so publishers, replies and declarations serialise there and wait.
The only sender that takes the TX mutex directly is the lease task's keep-alive
(`_z_transport_tx_try_send_t_msg`, `unicast/lease.c`). A put therefore drops
only when it lands during a keep-alive write. That is rare, which is why 1980
saw it in 1 run of 25, and only under CPU load.

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
3. **No fork change: zpico reports every failed BEST_EFFORT put as success.**
   Cheap, but a best-effort publisher on a dead session then also succeeds,
   so a publish result stops revealing a dead session.
4. **Leave it.** It is user-visible today. nros-c's action server publishes
   feedback BEST_EFFORT (`packages/api/nros-c/src/action/server.rs`), so
   `rclc_action_publish_feedback_raw` returns `RCL_RET_ERROR` whenever another
   thread holds the TX mutex. Any `RCLC_QOS_SENSOR_DATA` publisher does the
   same.

Scope of option 1, measured in the pinned zenoh-pico (1.8): declarations take
the mutex with BLOCK, query requests default to BLOCK
(`z_internal_congestion_control_default_request`), and a reply follows its
query. So a DROP send is a put, a delete or an admin-space put, which is the
best-effort data plane and nothing else.

Option 1 is the recommendation: it is the zenoh behaviour, and it removes the
error rather than giving it a name.

## Resolution (2026-10-23)

Option 1, chosen by the maintainer. The zenoh-pico fork's `nano-ros` branch
gained `57bbde9e fix(nano-ros): a DROP put dropped for congestion reports
success`. In `_z_transport_tx_send_n_msg` (unicast and multicast) and in
`_z_raweth_send_n_msg`, a try-lock that fails under DROP logs the drop and
returns `_Z_RES_OK` instead of the try-lock's error. Only DROP reaches that
branch, since BLOCK waits for the lock. A real send failure still propagates
from the send itself, so a dead session still fails a best-effort publish.

What a user sees: `rclc_action_publish_feedback_raw` and any BEST_EFFORT
publisher no longer report `RCL_RET_ERROR` / `PublishFailed` for a sample the
QoS allowed to be lost, matching zenoh and `rmw_zenoh_cpp`. Such a drop is
counted nowhere; zenoh-pico logs it at INFO only.

**No regression test, deliberately.** A first test hammered BEST_EFFORT puts
from four threads on one session and passed WITHOUT the patch (582,245 puts,
0 failures). That is because of the serialisation above: publishers never
contend for the TX mutex with each other, so the test never reached the
patched branch. The only contender is a keep-alive, which fires only when the
link has been idle and cannot be timed from a test. A test that cannot fail
is worse than none, so it was not kept. The change is the two-line branch in
the fork commit; review it there.
