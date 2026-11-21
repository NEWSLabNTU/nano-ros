---
id: 2799
title: "A threadx-linux talker exits with status 1, printing nothing, when its router is killed mid-session"
status: open
type: bug
area: [threadx, zenoh, testing]
severity: medium
found: 2026-11-21
related: [2779, 2774]
---

## What fails

The issue-2779 reopen cell for threadx-linux
(`rtos_e2e::test_rtos_listener_reopens_after_its_router_restarts::platform_2_Platform__ThreadxLinux`)
runs the platform's own Rust talker and listener. It waits for one sample,
SIGKILLs the router, keeps it down 5 s, restarts it on the same port, and waits
60 s for another sample (`nros_tests::router_outage`).

The listener is not what fails. In every red run the TALKER process ends when the
router dies:

    [INFO] Publishing: 'Hello World: 2'
    nros run: the target exited with code 1 after 2.1 s

Nothing else is printed: no `spin_once error`, no `Application error`, no
panic text. With the talker gone, the listener has nothing to hear after the
restart, and the cell reads "none in the 60 s after it came back".

Measured on newslab 118, with both images built by a direct `nros build`
(no rustc wrapper, so no sccache-served rlib) and the TCP read checked
with objdump:

| `nros-platform-threadx/src/net.c` | runs | talker exits at the outage | cell |
| --- | --- | --- | --- |
| issue 2779's fix (`NROS_PLATFORM_NET_SOCKET_EOF`) | 5 | 1 (run 5) | 4 pass, 1 fail |
| before the fix | 3 | 3 (about 3.1 s after start) | 3 fail |

In the four passing runs the talker survives the outage, and the listener hears
a sample 1.8 s after the restart.

## What is known about the exit

- Status 1 with no message rules out the board's signal path, which exits with
  `128 + sig` (`board_threadx_linux.c`).
- `BoardExit::exit_failure` on threadx-linux is libc `exit(1)`, which flushes
  stdout. Every `B::exit_failure()` in `nros-board-threadx/src/entry.rs` prints
  its reason first, so a reason printed on that path would have appeared.
- `nros_platform_panic` also ends in `exit(1)`. Its text goes through the
  registered log writer, so whether that text reaches the captured output is
  unverified.

So the cause is unknown. It is not the EOF read issue 2779 fixed: with that
fix in place the exit still happens 1 run in 5, and without it every run. A node
whose router dies is meant to keep running and reopen, as the passing runs do
and as issue 2774 established for POSIX.

## Evidence

Kept on newslab 118 in `nros-wt-2779b`:

- `build/issue-evidence-threadx-exit/pos-5.log`: the failing run with the fix.
- `build/issue-evidence-threadx-exit/neg-1.log`: an unfixed run.
- `build/failed-run-logs/*KJT382*talker*` and `*Q86G2B*listener*`: the console
  logs of the failing fixed run.

## Closes when

The exit has a named cause. The talker either survives the outage or reports
why it stopped, and the threadx-linux reopen cell passes 10 of 10.
