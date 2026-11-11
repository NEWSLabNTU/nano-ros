---
id: 2420
title: "`rmw_coordinate_truth`: the freertos-posix zenoh C entry once refused its session after waiting for its baked router port"
status: open
type: bug
severity: low
area: [tests, rmw-zenoh, freertos]
related: [2416, 1819, 1933]
found: 2026-11-10
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
