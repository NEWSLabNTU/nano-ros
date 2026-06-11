---
id: 29
title: XRCE session spin uses POSIX CLOCK_MONOTONIC — breaks bare-metal builds
status: resolved
type: bug
area: rmw-xrce
related: [issue-0026]
resolved_in: fa9228d23
---

**RESOLVED (`fa9228d23`).** `82d3c9763` (drive_io pacing for the [issue 0026](0026-px4-xrce-bare-agent-type-matching.md)
discovery race) added a `clock_gettime(CLOCK_MONOTONIC)` / `nanosleep`
busy-wait to `nros_rmw_xrce` `session.c` (`nros_rmw_xrce_session_spin`). POSIX
`CLOCK_MONOTONIC` is not declared in the bare-metal Cortex-M `<time.h>`
(thumbv7m/thumbv7em) and there is no `nanosleep`, so **every bare-metal XRCE
fixture failed to compile**:

```
session.c:465:19: error: 'CLOCK_MONOTONIC' undeclared (first use in this function)
```

Surfaced on a full `build-test-fixtures` run — `qemu`, `freertos`, and
`threadx_linux` XRCE leaves all failed (`rc=2`).

**Fix.** Use the platform abstraction that already backs both hosted and
embedded targets — `nros_platform_time_now_ms()` + `nros_platform_sleep_ms()`
(`nros/platform.h`) — instead of the POSIX clock. Same pacing semantics (drive
the session across the `t`-ms window, ~1 ms yield on an early return).
Validated: `qemu-arm-baremetal/rust/talker-xrce` builds clean for
`thumbv7em-none-eabihf`.
