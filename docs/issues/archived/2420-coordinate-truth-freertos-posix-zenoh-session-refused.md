---
id: 2420
title: "`rmw_coordinate_truth`: the freertos-posix zenoh C entry once refused its session after waiting for its baked router port"
status: resolved
type: bug
severity: low
area: [tests, rmw-zenoh, freertos]
related: [2416, 1819, 1933, 2994, 2950]
found: 2026-11-10
resolved: 2026-11-27
resolved_in: "#4943 (issue 2994, release blocker B46): a connect a signal interrupted is finished, not failed"
---

## Measured

Scheduled tier-2 run 42767563994, job 129678696882 "tier 1 (cells)":
`rmw_coordinate_truth a_rows_entry_registers_its_nodes_at_runtime` failed
in 48.6 s. Its junit (artifact `tier1-junit`) holds the panic:

```
port 9100: held by pid 1428289 (another test on the same baked port) — waiting up to 240s
rmw-coordinate-truth: ran 156 entries in 23.6s on a pool of 8 (49 cross-target row(s) left to their QEMU cells); ...
thread 'a_rows_entry_registers_its_nodes_at_runtime' panicked at packages/testing/nros-tests/tests/rmw_coordinate_truth.rs:986:5:
1 generated entry did not register with the peer this test started for it (issue 1819 — a missing peer is no longer an excuse):
  workspace-c-freertos-posix-zenoh (zenoh) — the backend refused the session although its peer was started for it
```

The same test passed in 37.1 s in run 42744194409 four hours earlier, so this
is intermittent, not a regression on main.

## What is known

- The failing row's image bakes its router locator (`baked_row_locator`):
  `examples/workspaces/c/src/demo_bringup/system.toml` `[image.freertos_posix_zenoh]`
  states `tcp/127.0.0.1:11130`. The test starts a router on that port. Nothing
  else in the tree names 11130.
- The `port 9100: held by …` line in the same panic is about a DIFFERENT row:
  9100 is the threadx-linux C pubsub port (`platform.rs` `THREADX_LINUX`), baked
  by `examples/workspaces/c/src/pubsub_bringup` and the threadx-linux C talker
  fixture row. The junit timestamps name the holder:
  `rtos_e2e test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_2_Lang__C`
  ran 07:27:40.7 to 07:28:53.3, and the coordinate-truth test ran 07:28:30.4 to
  07:29:19.0. No other test longer than 1 s overlapped.
- So two tests in one lane share a baked router port. The coordinate-truth test
  waits for it rather than colliding, which is why that row registered, but the
  wait is an isolation gap, not noise: it serialises the two tests on a port
  neither chose, and either one can meet the other's router if the wait ever
  misjudges the holder.
- The other 155 entries, the 9100 row among them, registered. The failing row
  runs under `BAKED_RUN_BUDGET` (10 s) and is judged on its setup line. A
  session that takes longer than that to open, for instance under the load of
  the 9100 wait and a pool of 8, would read as "refused".

## Not known

Whether the 11130 entry dialed before its router listened or simply ran out
of its 10 s budget under load. Its own output is not in the panic; a
recurrence should be read for it before changing the budget. Separately, the
9100 sharing between `rtos_e2e` and this test is a candidate for its own fix
(one owner per baked port per lane).

## The shared baked port: what is and is not a gap (2026-11-10)

A baked port is allocated per IMAGE COORDINATE (`alloc::port_of(platform,
lang, workload)`) and compiled into that image. Every test that runs the image
must use it, so "one baked port per fixture row" cannot hold. One binary is
one port. Sharing is by design: a sweep of `port_of(` callers in
`nros-tests/tests` finds coordinates read by several files. For example,
FreeRTOS C `EntryPubsub` is read by `entry_e2e` and
`pubsub_freertos_ros2_interop_e2e`, and `RealtimeSchedTasks` by
`realtime_sched_tasks_e2e` and `sched_dims_applied_e2e`. The 9100 case here is
the same thing.

What makes sharing safe is the per-port `flock` lease (`port_lease.rs`, issues
0470 and 2125). A fixture that starts a daemon on a caller-named port waits for
that port's lease and holds it for the daemon's lifetime, so two tests on one
baked port serialise and neither can reach the other's daemon. It works as a
nextest test-group per port, decided at run time, which a static test-group
could only approximate. The 9100 overlap in this run was that lease working:
`port 9100: held by pid …, waiting`.

The real gap is a starter that skipped the lease. Of the fixtures that bind a
caller-named port, every `ZenohRouter` starter (`start`, `start_on`,
`start_slirp`, `start_with_serial`, `start_tls`) takes it, but
`XrceAgent::start(port)` did not (`_lease: None`). Two tests running one
image's baked XRCE agent port started two agents on one UDP port, and their
clients could reach either one: issue 0470's cross-talk. It now waits for and
holds the UDP lease the same way, under one shared `NAMED_PORT_WAIT`.
Measured across processes: with the lease held by another process, `start`
printed `held by pid … waiting` and started 18.6 s later, when the holder
released it.

So the next starter cannot skip the lease, it is checked rather than remembered:
`port_lease::tests::every_caller_named_port_starter_takes_the_lease` reads the
fixture sources and fails on any public `start*` / `spawn*` starter taking
`port: u16` whose body neither takes the lease nor calls a fn that does.
Delegation counts only toward a callee that leases, so a public starter routed
through an unleased private helper is still caught. `ros_env::spawn_xrce_agent`
is exempt by construction: its port is `8000 + domain`, and the domain is
itself `flock`-leased for the process. Removing the lease from
`XrceAgent::start` makes the check fail (measured), and a synthetic negative
control covers the delegation cases.

Still open: part (2) above, the freertos-posix row's refusal on 11130. It is
read only on a recurrence, when #3456's lane log carries its output.

## Recurrence: tier-2 run 42928442297 (2026-11-11), at low load

The same row failed the same way in tier-2 run 42928442297 on `5e01d7ffed`,
in `test-all` (junit-real, artifact `post-submit-junit`). It was the run's
one real failure out of 3,519 tests:

```
rmw-coordinate-truth: ran 137 entries in 9.6s on a pool of 8 (59 cross-target row(s) left to their QEMU cells); slowest: ... 1.1s ...
  workspace-c-freertos-posix-zenoh (zenoh) — the backend refused the session although its peer was started for it
```

This rules two readings out. There was **no port-9100 wait** this time, and
the pool was not loaded: the slowest of 137 entries took 1.1 s, against the
row's 10 s `BAKED_RUN_BUDGET`. So neither the shared-port wait nor a budget
spent under load explains it. "Refused" means the entry printed the
session-open-failed marker and exited, within its budget.

The entry's own output was still not in the panic, because only an entry
that opened a session and then failed to register had its lines printed.
**Change (this PR):** every no-session verdict (refused, hung, did not reach
registration) now carries the entry's last 12 non-empty output lines,
indented under the verdict (`output_tail`, test
`a_no_session_verdict_carries_the_entrys_last_lines`). The next recurrence
will say why from the panic alone.

A reading to test then: the router is started and waited for until its TCP
port **accepts** (`wait_for_router_ready`), not until it completes a zenoh
handshake. The FreeRTOS-POSIX image dials once, because its connect budget
defaults to one attempt (issue 2207's knob,
`[image.<id>.knobs.zenoh.limits] connect_timeout_ms`). If that holds, a
handshake attempted while the router is still starting fails once and is
final. That is a start race to fix (CLAUDE.md: no retries), either with a
readiness edge the router reports or with this image's stated connect
budget. Not measured yet.

Two facts behind that reading, checked on main: `CONNECT_TIMEOUT_MS`
defaults to `0`, which is one attempt (`nros-rmw-zenoh/build.rs`, issue
2207). And `[image.freertos_posix_zenoh]` in
`examples/workspaces/c/src/demo_bringup/system.toml` states no
`connect_timeout_ms`, so it gets that one attempt. Only
`realtime-c`'s `smp_bringup` states one (issue 2384).

**Depends on issue 2389.** 139's `fix-2389-connect-budget` (not yet pushed)
rewrites this connect path: a bounded handshake per attempt, a non-blocking
connect, and a send stall. Its author confirmed (2026-11-11) that it **keeps
the one-attempt default**: it does not touch `resolve_zenoh_connect_timeout_ms`
(`nros-platform-config/src/platform_config.rs`), so `CONNECT_TIMEOUT_MS` still
resolves to `BUILTIN_ZENOH_CONNECT_TIMEOUT_MS = 0` unless the image or the env
states one. Its zenoh-pico fork commits (handshake `f84c6be5`, TCP connect
`6827fe1f`, write stall `15f3b012`) act only on a positive budget; with `0`
they pass no deadline and the open is one attempt, as on main. `-1`
(forever) is unchanged too. So the reading stands after 2389. What would
close this is either the image's own `connect_timeout_ms` (a retry window,
which 2389 then bounds correctly) or a router-readiness edge in the
harness. The next recurrence's output, now printed, should say which.

## Not reproduced in 50 runs (2026-11-22)

`a_rows_entry_registers_its_nodes_at_runtime`, run 50 times in a row on main
(`332dbcd998`) with only the `workspace-c-freertos-posix-zenoh` row built, so
every run started that row's baked router on 11130 and judged that row alone.
All 50 passed. The entry registered in about 110 ms each time, against its
10 s `BAKED_RUN_BUDGET`, on a host at load 15 to 23 of 24 cores. That rules
out a failure at any measurable rate in isolation. It does not rule out the
start race the reading above names, which needs the router's start and the
image's single dial to meet; a loaded lane is where that happened both times.
No fix is made without a failing run to read. The next recurrence prints the
entry's own output tail, which is what decides between the two remedies.

The `threadx-linux` hang seen in the same test on 2026-11-22 was a different
failure, and not this issue's. That entry reached its first spin and never
registered, and it came from a PR's own wait change: the same steps on that
PR's base passed.


## No recurrence of this shape (2026-11-25)

`a_rows_entry_registers_its_nodes_at_runtime` failed in all three lanes of
run 45351389552 (main `0d9fa3e470`, 2026-11-24), but not with this issue's
shape. No session was refused after a baked-port wait. Two other rows,
`workspace-features-c-params` and `workspace-features-mixed-params`, stopped
before spinning with "component setup failed with code -6/-7", the same in
every lane: a deterministic setup failure, not this start race. It passed at
`730dfbebeb` (run 45422927491), so a change between the two heads fixed it.
The freertos-posix zenoh row registered in every run read. This stays a watch
item; the next recurrence of the refused-session shape is what decides
between the two remedies above.

## Resolution (2026-11-27): issue 2994's EINTR, not a start race

The cause is [issue 2994](2994-freertos-posix-zenoh-connect-eintr.md),
fixed for release blocker B46 by #4943 (`ffa575c7a2`, with issue 2950's
`679add46e1`, zenoh-pico fork pin `4e93fb95`).

It is the same image (`workspace-c-freertos-posix-zenoh`) and the same baked
port (11130). The FreeRTOS POSIX tick is a SIGALRM without `SA_RESTART`,
and Linux never restarts a `connect()` on a socket with `SO_RCVTIMEO` set.
So a tick landing inside the one-attempt connect returned EINTR, which the
zenoh-pico helper reported as a refusal. That fits everything above:

- the open failed at once, within budget, while the router was listening
  (2994 measured `[0.002]`);
- it happened only in loaded lanes, where a tick is likelier to land inside
  the call, both times;
- it never happened solo (50 of 50 on 2026-11-22).

The start race this issue suspected is ruled out by 2994's measurement. A
router stopped after it listened makes the image fail only at the 10 s
handshake bound, not in milliseconds.

Verified on host 241 with this issue's own test at main (B46 in, fork pin
`4e93fb95`), `rmw_coordinate_truth a_rows_entry_registers_its_nodes_at_runtime`,
with the `workspace-c-freertos-posix-zenoh` row freshly built:

| run | result |
| --- | --- |
| plain | 10 of 10 pass; the row registers in about 140 ms |
| an `LD_PRELOAD` shim makes the image's first TCP `connect()` report EINTR | 10 of 10 pass; the shim fired in each run (logged per pid) |

Under the shim the real call had returned EINPROGRESS: issue 2950 made the
one-attempt connect non-blocking. So the injected EINTR is a signal landing
in a connect that is still going on. The fixed helper waits for writability
and reads `SO_ERROR`, so the same attempt completes. Issue 2994 measured the
other direction on the same image: before the fix, one injected EINTR failed
the open with rc 156 in 2 ms, the nightly's exact line. Its offline check is
`just check zpico-connect-eintr`.
