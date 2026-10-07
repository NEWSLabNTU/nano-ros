---
id: 1732
title: "A native image exits without closing its RMW session, so an XRCE Agent keeps its participant — topics, services and nodes — after it is gone"
status: open
type: bug
area: rmw, xrce, boards
severity: medium
found: 2026-10-07
related: [issue-1292, phase-480]
---

## What happens (measured, 2026-10-07)

`examples/workspaces/rust`'s `native_xrce_entry` (row
`workspace-rust-native-xrce`), the pinned Agent `2.4.3-nros1` on `udp4 -p
27911` with the issue-1009 loopback profile, domain 148, a stock Humble
`rmw_fastrtps_cpp` peer on the same profile:

    NROS_ENTRY_SPIN_MS=8000 native_xrce_entry   -> "nros: application complete", rc 0
    during the run   ros2 node list --no-daemon  -> /listener /talker
    after it exits   ros2 node list --no-daemon  -> /listener /talker   (2 of 2 samples)
                     ros2 topic list --no-daemon -> /chatter /parameter_events /rosout

The image is gone and the peer still sees its nodes and its `/chatter`. The
same happens to a SIGTERM'd XRCE image (`service-server` killed by `timeout`:
`/add_two_ints` and `/add_two_ints_server` both stay listed).

Control on the same Agent: a client that DOES call `uxr_delete_session` before
it exits (the issue-1292 prototype, domain 146) leaves nothing — `node list`
empty, `topic list` only `/parameter_events /rosout`. So the Agent removes a
participant when its session is deleted, and only then: Agent 2.4.3 has no
client liveliness timeout unless built with `UCLIENT_HARD_LIVELINESS_CHECK`,
which the client is not.

## Why (read from the source, not traced)

`nros-board-linux`'s single-executor run path ends
`<Self as BoardExit>::exit_success()` with the `ExecutorNodeRuntime` (and the
`Executor` inside it) still alive in `crt_real`
(`packages/boards/nros-board-linux/src/lib.rs`, after "application
complete"). An exit that skips the executor's drop never reaches
`Executor::close` -> `destroy_session`, so `xrce_session_destroy` never sends
`uxr_delete_session`, and the Agent keeps the client's DDS participant with
every endpoint on it.

Zenoh and Cyclone hide this: a vanished zenoh session's liveliness tokens and
a vanished Cyclone participant both expire by lease. XRCE has no lease between
client and Agent, so on XRCE the leftovers are permanent until the Agent
restarts.

## Why it matters more since issue 1292

Before 1292 the leftovers were endpoints attributed to
`_CREATED_BY_BARE_DDS_APP_`. Since 1292 the Agent's participant also carries
the image's `ros_discovery_info` sample (TRANSIENT_LOCAL), so a dead image's
NODES stay in `ros2 node list` too. Every XRCE cell that shares one Agent
across runs, or a fixed domain, can read a previous run's nodes. The live cell
`native-multinode-rust-xrce` avoids it only because it starts its own Agent and
picks its own domain.

## What a fix needs

- Close the session on every normal exit path of the native boards (drop or
  `close()` the executor before `exit_success` / `exit_failure`), and say
  whether the RTOS boards share the shape.
- A SIGTERM'd process still cannot close anything. Whether to build the client
  with `UCLIENT_HARD_LIVELINESS_CHECK` (or rely on the Agent operator) is a
  separate decision; measure the Agent's behaviour first.

## Not measured

Whether `nros-board-linux`'s tier path and the C/C++ native entries have the
same shape. Whether any RTOS board exits at all. The fix.
