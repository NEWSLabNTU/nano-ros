---
id: 2181
title: "A C++ image on Cyclone logs `destroy_publisher failed with 1` at SIGTERM"
status: resolved
resolved_in: "2026-10-31"
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

## Cause

Traced under gdb on the cpp talker. `rclcpp::shutdown()` → `nros_cpp_fini` →
`Executor::close` releases the nodes and then calls `session_destroy`, which
deletes the participant, and Cyclone deletes the participant's writers, readers
and topics with it. `~Publisher()` runs later, at the end of `main`, and
`publisher_destroy`'s `dds_delete(writer)` gets a handle Cyclone no longer knows
(`dds_handle_pin_for_delete`: BAD_PARAMETER, or PRECONDITION_NOT_MET once the
library has deinitialised). No entity is destroyed twice and nothing leaks; the
slot reported ERROR for an entity that was already gone. A C image destroys its
entities before it closes the session, so it never meets this.

## Resolution

`delete_owned_entity()` (`nros-rmw-cyclonedds/src/session.cpp`) counts only the
three pin-failure codes as "already gone" and returns every other code, so a
deletion that was attempted and failed still reports. It is used at every
non-`(void)` `dds_delete` in the destroy slots: publisher, subscription, service
and client. `node_destroy` runs before `session_destroy`
(`release_session_nodes`), so it never meets this case.

## Measured

- cpp talker: no destroy line. Reverting the helper to return the raw rc brings
  `destroy_publisher failed with 1` back.
- `examples/workspaces/mixed` `linux_cyclonedds`: no destroy line on 2 runs of 2,
  5 messages received each.
- `just check rmw-cyclonedds`: 38/38.

## Not measured

- Whether zenoh or XRCE C++ images report the same thing at teardown (neither
  was observed to).
