---
id: 1723
title: "`timeout N ros2 …` does not bound a ros2 CLI that is waiting: it ignores the SIGTERM, so the deadline never fires"
status: resolved
type: bug
area: testing
severity: medium
found: 2026-10-06
related: [issue-0659, issue-1139, 1741, phase-480]
---

## What happens

GNU `timeout N cmd` sends `SIGTERM` at the deadline, waits for the child, and
sends `SIGKILL` only when given `-k`. A waiting `ros2` CLI does not always die
on `SIGTERM`. Measured 2026-10-06 while working on issue 1352:

- `timeout 25 ros2 service call /param_talker/set_parameters …`, against a
  server that dropped the request, was still running **20 minutes** later.
  Only `SIGKILL` ended it. `timeout -s KILL 25` bounds the same call at 25 s.
- On the same host, four `timeout 20 ros2 topic echo … /diagnostics`
  processes started on 2026-10-03 were still alive three days later, each
  holding a DDS/zenoh participant. They are not from this work.

## Why it matters

Several harness sites rely on `timeout N ros2 …` as the bound on a peer:
`tests/entry_e2e.rs`, `tests/interop_e2e.rs`, the three `*_ros2_interop_e2e.rs`
cells, `docker/can-demo/entrypoint.sh`, and the `bash -c "<env> && timeout N
ros2 run …"` chain that `process.rs` describes. When the CLI ignores the
signal, the cell hangs until something outside it gives up, and the orphan
keeps a participant on the bus. Issue 0659 is that same orphan, reached by a
different path.

It is also a candidate cause for issue 1139's unexplained wall clock (one
in-gate pass took 199.7 s against a typical 3 s), but nothing here shows it
caused that run.

## Not established

- Which `ros2` verbs ignore `SIGTERM`, and when. Seen with `service call`
  waiting for a reply and with `topic echo`. rclpy installs its own signal
  handling, so this may depend on where the CLI is when the signal lands.
- Whether `-k` (a kill-after grace) or `-s KILL` is the right spelling for the
  harness. The class fix is ONE helper that every site uses, not a flag added
  at each site.

## Sweep

```
git grep -nE "timeout [0-9\"\$A-Z_{}]+ +(env [^|]*)?ros2 " | grep -v -e "-k " -e "-s KILL"
```

## Resolution

Resolved 2026-10-07 on `fix/1723-ros2-deadline-escalates`.

**Why, measured** (Humble, this host). rclpy installs a SIGTERM handler once
`rclpy.init()` returns (`SigCgt` of every surviving CLI had bit 15 set), and
that handler does not end the process. One SIGTERM to a running
`ros2 topic echo --no-daemon`, sent 1 / 3 / 8 s after start:

| RMW | survived |
| --- | --- |
| `rmw_zenoh_cpp` 0.1.9 (live router) | 3 of 3 |
| `rmw_cyclonedds_cpp` | 0 of 3 (exit 15) |
| `rmw_fastrtps_cpp` | 0 of 3 |

(At 0.3 s, before `rclpy.init` returns, all three die: rc 143.) A
`faulthandler` stack of a survivor is in `rclpy.spin` →
`_wait_for_ready_callbacks`. A SECOND SIGTERM kills it.

GNU `timeout` (coreutils 8.32 here) never escalates without `-k`, and the
two spellings differ in how many SIGTERMs they send (strace: plain
`timeout` signals the child AND then its own process group; `--foreground`
only the child):

- `timeout --foreground 5 ros2 topic echo` (the Rust harness spelling,
  30 sites): 3 of 3 still alive at 25 s;
- `timeout 5 ros2 topic echo` (daemon-spawning): 4 of 10 alive at 25 s — the
  second signal usually lands; reasoned, not traced, that the misses are the
  two signals coalescing while pending;
- `timeout --foreground -k 2 5 …`: gone at 7.1 s, 3 of 3.

So the hang is rclpy's handler meeting a `timeout` that sends one SIGTERM;
the ros2cli daemon and the process group were not the cause (the daemon is
`setsid`'d and outlives every spelling by design).

**Fix, one spelling per language:**

- Rust: `nros_tests::ros2::ros2_deadline(secs)` =
  `timeout --foreground --kill-after=3s <secs>` (`--foreground` so the
  harness keeps owning the process group and its orphan ledger).
- Shell: `scripts/lib/ros2-deadline.sh` → `"${NROS_ROS2_DEADLINE[@]}" N …`
  (no `--foreground`: a script has no group owner, so `timeout`'s own group
  kill takes `ros2 run`'s node down too). The can-demo image gets the file
  through a `nroslib` build context.
- 57 sites moved: 30 in `nros-tests/src`, 13 in `nros-tests/tests`, 14 shell
  lines (can-demo, the Cyclone e2e scripts, isotp, probe, debug).
- Gate `check-ros2-cli-deadline` (fast line): a literal `timeout` in shell
  position whose command is `ros2`/`python3` (after `env`, `stdbuf`,
  assignments, `\` continuations joined) is refused, and the two graces are
  held equal; self-test on the normal path.

Tests: `ros2::deadline_tests` — a TERM-ignoring stand-in is ended by the
spelling (4.0 s for a 1 s deadline + 3 s grace); the NEGATIVE CONTROL keeps
the same stand-in alive past 6 s under the old `timeout --foreground 1`; and
with `--kill-after` removed from `ros2_deadline` the positive test FAILS
(measured).

Orphans: the four `timeout 20 ros2 topic echo … /diagnostics` processes from
2026-10-03 (PWD in another agent's nano-ros worktree, zenoh session config
under its `tmp/ws1635`) were killed with SIGKILL on 2026-10-07. A
`timeout 6 …/threadx-linux/c/service-server/build-cyclonedds/c_service_server`
from 2026-09-03 is the same class one layer over and was left running and
filed as issue 1741.

Sweep: `python3 scripts/check-ros2-cli-deadline.py`, and
`git grep -nE "timeout [-0-9\"\$A-Za-z_{}]+ +(env [^|]*)?(ros2|python3) "`

**Not measured.** Jazzy or any other distro; `ros2 service call` against a
server that drops the request (the issue's case — the measured verbs were
`topic echo` and `service call` with no server); the can-demo image (not
built); 1139's 199.7 s wall clock (nothing here shows this caused it).
