---
id: 2243
title: "The nightly PX4 SITL job's two booting tests start two PX4 daemons in one build dir at once"
status: resolved
type: bug
area: [px4, testing, ci]
severity: medium
found: 2026-11-04
related: [phase-466, phase-521, phase-542]
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-11-04: the nightly PX4 SITL job's red comes from the harness, not the code under test; phase-521, which added the job, is archived. [Phase-542](../../roadmap/phase-542-px4-supported-platform.md) W5 (the job as a `daily` lane) depends on the fix.

## Measured

The first scheduled run of the `px4 sitl` nightly job, added by
`30ea9fdf71` (phase-521 W8e), failed: nightly run 41790118101 (2026-11-04
07:07 UTC), job 126441270395, step `Build SITL and run the PX4 cells`:

    PASS [  20.385s] (2/3) nros-tests::px4_sitl work_queue_tier_wakes_only_for_deadlines_and_arrivals
    FAIL [  30.063s] (3/3) nros-tests::px4_sitl stock_px4_listener_reads_a_nano_ros_publication

    PX4 SITL did not boot: 0 of 1 line(s) containing `Startup script returned successfully` within 30s
    === SITL log ===
    (empty)

`tests/px4_sitl.rs` calls `Px4Sitl::boot_in(&build_dir)` from both
`work_queue_tier_wakes_only_for_deadlines_and_arrivals` (line 60) and
`stock_px4_listener_reads_a_nano_ros_publication` (line 137). Both use the
same `build_dir`, and both run `bin/px4 -d etc/init.d-posix/rcS` with no
instance argument. Nothing serialises them: no `Mutex` in the test file, and
no `.config/nextest.toml` override for `px4_sitl`. So nextest runs both
tests at once.

The timings fit two PX4 daemons racing as instance 0. One finished its test
in 20 s. The other waited out its 30 s boot timeout without its SITL printing
a single line. PX4's posix daemon takes a per-instance lock and fixed UDP
ports.

### Second sighting: the other test lost (2026-11-05)

Nightly run 41985062052 (2026-11-05 07:07 UTC, main `1cc4ad92`), job
127103760109, step `Build SITL and run the PX4 cells`. The roles were reversed:

    FAIL [  30.026s] (3/3) nros-tests::px4_sitl work_queue_tier_wakes_only_for_deadlines_and_arrivals
    PX4 SITL did not boot: 0 of 1 line(s) containing `Startup script returned successfully` within 30s

On 2026-11-04 `work_queue_tier_…` passed and `stock_px4_listener_…` failed.
On 2026-11-05 `work_queue_tier_…` failed. The failing test changes from run
to run, and each time the SITL log is empty and the failure is the 30 s boot
timeout. A defect in either test's own logic would fail the same test every
time; a race between the two boots fails whichever one loses.

## What it is not

- **Not issue 1896** (the PX4 bridge's XRCE client colliding with
  uxrce_dds_client): that failure was a booted SITL with a broken bridge. Here
  the second SITL never prints.
- **Not proven.** Two daemons is the reading the code and the timings support.
  The SITL's own stderr was empty, and no `px4_lock` or port error was
  captured. Running the failing test alone with `-j1` would confirm it.

## What would close it

The fix sites:

- A nextest `test-group` with `max-threads = 1` for `px4_sitl`, the way other
  shared-daemon tests in `.config/nextest.toml` are grouped.
- Or a process-wide lock in `Px4Sitl::boot_in`.
- Or a distinct instance per test (`-i <n>`). That needs the uORB peer to know
  the instance's ports, so it is the larger change.

Acceptance: two consecutive nightly `px4 sitl` runs pass all three cases.

## Resolution (2026-11-10)

### Confirmed

The third and fourth scheduled sightings have the same shape: nightly run
42577453343 (2026-11-09), job 129028272614, `work_queue_sched_task_...` PASS in
20.2 s and `stock_px4_listener_...` FAIL at the 30 s boot timeout with an empty
SITL log. Reproduced locally on the pinned PX4 v1.17.0 SITL with the unfixed
harness: the same split, 2 passed and 1 failed.

The resource is wider than the issue said. PX4's posix daemon keys its server
lock (`/tmp/px4_lock-<n>`) and its client socket (`/tmp/px4-sock-<n>`) on the
INSTANCE, under a hardcoded `/tmp` (`platforms/posix/src/px4/common/main.cpp`,
`px4_daemon/sock_protocol.cpp`). So instance 0 is one resource per HOST, not
per build dir: two SITL trees in two checkouts collide the same way. The lock
itself is taken in two steps (`F_GETLK`, then `F_SETLK` after setup), and the
server `unlink`s the socket path before it binds, so two daemons started
together can both pass the check, and the loser dies before it prints a line.
A `px4-<module>` client also reaches whichever server bound the socket last,
so a second daemon would have broken the first test's `shell()` calls even
when it booted.

### Fix

The class is "a second PX4 daemon on one instance", and the harness now makes
that impossible for anything it boots:

- `Px4Sitl::boot_in` takes an exclusive `flock` on
  `/tmp/nros-px4-sitl-instance-0.lock` BEFORE it spawns the daemon, and holds
  it until the daemon is dead (struct field order is drop order). A second
  `Px4Sitl`, in another test, process or nextest slot, waits for it (up to
  300 s); the 30 s boot timeout starts after the wait. The lock is a separate
  file: an `fcntl` lock on PX4's own file would read to PX4 as a running server.
- With the lock held, a daemon nobody in the harness started (a hand-run
  `bin/px4`) is detected by connecting to `/tmp/px4-sock-0`, and the test fails
  naming it instead of racing it for the socket.
- `.config/nextest.toml` puts `binary(px4_sitl)` in a `px4-sitl-instance`
  group with `max-threads = 1`. That is not the guarantee: it only keeps the
  lock's wait out of each test's 60 s budget.

Per-test instances (`-i <n>`) were not taken: PX4 then needs a per-instance
working directory (`-w`), every `px4-<module>` call needs `--instance <n>`, and
the gain is about 20 s of one nightly job.

### Evidence

All on the box, against `just px4 build-sitl-example`'s tree:

| run | config | result |
| --- | --- | --- |
| unfixed harness, no group | as the nightly runs it | 2 passed, 1 failed (`stock_px4_listener_...`, 30 s boot timeout, empty log) |
| fixed harness + group, x3 | the merged state | 3 passed each time, 22.6 to 23.1 s |
| fixed harness, group removed, x3 | lock alone, both booting tests started together | 3 passed each time; one test's 23.0 s includes its wait for the other |
| a hand-started `bin/px4` daemon on instance 0, then `stock_px4_listener_...` | the foreign-daemon check | fails at once: "a PX4 daemon not started by this harness is serving instance 0" |

The acceptance this issue named, two consecutive green scheduled `px4 sitl`
runs, is phase-542 W5's to observe: W5 needs seven.
