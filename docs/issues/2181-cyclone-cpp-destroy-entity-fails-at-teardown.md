---
id: 2181
title: "A C++ image on Cyclone logs `destroy_publisher failed with 1` at SIGTERM"
status: open
type: bug
area: [rmw-cyclonedds, cpp]
severity: low
found: 2026-10-31
related: [1769, 2159]
---

## What was measured

Hosted (`linux`, CMake 3.22.1), Cyclone, each image run for 4 s and stopped by
`timeout` (SIGTERM):

| Image | Teardown line |
|---|---|
| `examples/linux/cpp/talker`, copied, `rmw = "cyclonedds"` | `[ERROR] nros: destroy_publisher failed with 1; the backend may have leaked the publisher` |
| a C `c_talker` over the same `std_msgs`, same configure | none |
| `examples/workspaces/mixed` `linux_cyclonedds` (C server + C++ client + pub/sub) | `destroy_subscription failed with 1; the backend may have leaked the subscription` |

The message comes from `packages/rmw/cffi/src/lib.rs` (`destroy_publisher` /
`destroy_subscription`), where a non-zero return from the backend's destroy slot is
reported rather than propagated. Each image published or delivered normally
before teardown.

Issue 1769 recorded the same `destroy_subscription failed with 1` line on
freertos-posix with the host Cyclone, "before and after" that fix, and left it
unfiled.

## Not measured

- Whether the C++ path destroys an entity twice (a C++ RAII destructor plus the
  executor's own teardown) or Cyclone refuses a destroy after its participant is
  gone — the order of teardown was not traced.
- Whether zenoh or XRCE C++ images report the same thing.
- Whether anything actually leaks: the process is exiting.
