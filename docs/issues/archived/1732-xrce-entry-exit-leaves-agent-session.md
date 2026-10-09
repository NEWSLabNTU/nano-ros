---
id: 1732
title: "A native image exits without closing its RMW session, so an XRCE Agent keeps its participant — topics, services and nodes — after it is gone"
status: resolved
type: bug
area: rmw, xrce, boards
severity: medium
found: 2026-10-07
related: [issue-1292, issue-1741, issue-1762, phase-480]
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

## Resolution

Fixed on `fix/1732-native-image-closes-session` (commit "a native image
closes its RMW session on every exit path", plus its regression test).

**Cause, confirmed by measurement.** Two separate exits skipped
`destroy_session`:

- the Rust board's `boot_hosted` called `std::process::exit` with the executor
  still alive, so no destructor ran (the source reading above);
- SIGTERM and SIGINT killed every native image by the default action. That
  covers Rust, C and C++ entries.

Measured on main (pinned Agent `2.4.3-nros1`, `workspace-rust-native-xrce`,
`ros2 node list --no-daemon`):

- clean exit: rc 0, `/listener /talker` still listed;
- SIGTERM: rc 143, both still listed.

**Fix — one guard, every hosted funnel.**

- `nros-platform-posix`: `nros_posix_install_termination_guard` and
  `nros_posix_termination_requested`.
  - The handler only sets a `sig_atomic_t`; nothing is torn down in a signal
    frame.
  - It is installed only over `SIG_DFL`. An application's own handler and
    `SIG_IGN` are left alone.
  - It uses `SA_RESTART`.
  - A second signal ends the image by the default action, the contract issue
    1741 gave threadx-linux. That guard is a board-local sigwait thread,
    because the ThreadX scheduler owns the mask, so it cannot serve these
    boards. Its naming is kept.
- `nros_platform::termination::{install_guard, requested}`: the one Rust
  spelling.
- `nros-board-linux`: `close_session` runs `Executor::close` and then the drop,
  the order `nros_cpp_fini` uses. It runs before `exit_success` and
  `exit_failure`. The tiered path's spins end on the flag, `thread::scope`
  joins them, and the boot executor closes the session.
- `nros::main!` hosted spin loops, both forever and bounded, return on the flag.
- `nros-cpp`:
  - the C-ABI runners (`run_components_named_in`, which the generated C entry
    calls, and `run_tiers_in`) install the guard and leave their loops on it;
  - `nros_cpp_spin_for` honours it;
  - the header runner `LinuxBoard::run_components` (the generated C++ entry)
    installs it through `nros_cpp_termination_guard_install`;
  - `nros::ok()` folds in `nros_cpp_termination_requested`, which is
    `rclcpp::ok()`'s contract.

**Measured after (same Agent and query).**

| entry | clean exit | one SIGTERM |
| --- | --- | --- |
| Rust `workspace-rust-native-xrce` | rc 0, listing empty | rc 0, listing empty |
| C `workspace-c-native-xrce` | rc 0, listing empty | rc 0, listing empty |
| C++ `workspace-cpp-native-xrce` | rc 0, listing empty | rc 0, listing empty |

On the C++ entry, before its header half landed (Rust half only), SIGTERM still
gave rc 143 with both nodes listed. The C++ entry's clean exit was already
closing the session (`nros::shutdown()` → `nros_cpp_fini`).

**Zenoh and Cyclone: no visible gap, measured.** Rust workspace entries, a
local `rmw_zenohd` and a loopback-pinned Cyclone peer. After SIGKILL, SIGTERM
or a clean exit, `ros2 node list --no-daemon` was empty at +0.7 s on both.
The reasons:

- the zenoh router drops a session's liveliness tokens when its TCP link
  closes;
- a fresh Cyclone participant cannot discover a participant that has stopped
  announcing.

Only XRCE holds a client's participant on its behalf. The close is still
graceful on both now.

**Regression test.** `rust_multi_node_per_node_graph::
rust_multi_node_entry_leaves_the_graph_when_it_ends_xrce` covers both endings
in sequence, each with its own Agent. It PASSES with the fix (24.7 s, solo).
With `close_session` and the guard disabled and the fixture rebuilt, it FAILS:
`/listener /talker` are still listed after the clean exit. It is bound to the
existing `native-multinode-rust-xrce` cell.

Writing it found issue 1762: two parallel cases drew the same domain from
`unique_ros_domain_id`. The test checks that its domain is quiet instead of
assuming it.

**RTOS boards (read, not measured).** Their run paths spin forever and have
no process to end, so this class should not reach them. threadx-linux ends on
SIGTERM through issue 1741's own guard.

Sweep: `git grep -n "process::exit\|exit_success()\|exit_failure()\|spin_once\|nros_cpp_fini" -- packages/boards/nros-board-linux/src packages/api/nros-cpp/src packages/api/nros-cpp/include/nros/main.hpp packages/core/nros-macros/src/main_macro.rs`.

## Not measured (at resolution)

- Whether threadx-linux's guard (issue 1741) closes the session. It calls the
  application's handler or ends the image; that is outside this issue's native
  boards.
- The C entry before the fix. It shares the C-ABI runner, but it was not built
  on main.
- A panic inside `setup` or a spin. A panic still skips the close.
- Hand-written C examples (`examples/native/c/*`) that install their own
  `signal()` handlers. The guard leaves them alone by design, and whether
  their own teardown closes the session was not checked.
- zenoh over a link with no FIN (lease expiry), and Cyclone through a daemon
  or a long-lived peer that caches a dead participant until its lease ends.
