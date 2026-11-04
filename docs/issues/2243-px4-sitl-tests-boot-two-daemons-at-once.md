---
id: 2243
title: "The nightly PX4 SITL job's two booting tests start two PX4 daemons in one build dir at once"
status: open
type: bug
area: [px4, testing, ci]
severity: medium
found: 2026-11-04
related: [phase-521]
---

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
