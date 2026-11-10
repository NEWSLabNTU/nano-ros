---
id: 2352
title: "entity_wake's arrival-vs-drain race test hangs to its 60 s timeout when it runs beside the build-tier gates"
status: open
type: bug
area: [testing, executor, rmw-zenoh]
severity: medium
found: 2026-11-07
related: [2221, 1980, 1997, phase-543]
---

**Home: [phase-543](../roadmap/phase-543-nextest-gates.md).**

## Measured

phase-543's order-independence check (`just check gate-order build`) runs the
build tier twice, starting the tests in opposite name orders. On 2026-11-07
(lane `dev`, host newslab-118, 48 cores), `build::check::required-features-tests`
failed in the ascending run and passed in the descending one. Every other build
gate had the same verdict in both orders.

The red inside that gate was one test, in the `nros-tests` feature lane
(`--features trigger-test,component-runtime-test,phase216-substrate,signal-fd-wake-test`):

    TIMEOUT [  60.026s] nros-tests::entity_wake
        an_arrival_racing_a_drain_is_never_left_undispatched::case_2_one_sample_per_dispatch

The same test, from the same build:

- alone, 8 runs: 8 passes, about 0.48 s each;
- 48 copies run at once, both cases: 48 of 48 pass, twice.

So plain CPU load is not enough to trigger it. The failing run had the other
build gates beside it, compiling and running their own test binaries and
routers, and which gates ran beside it is exactly what changes between the
two orders.

## What it is (not yet known)

The test opens its own router (`zenohd_unique`), its own ROS domain, and an
executor that races an arrival against a drain. A 60 s hang, not a wrong
count, suggests either a router that never becomes ready under that load, or
a wake that is lost so the executor parks forever. It is not a retry
candidate (no retries, maintainer ruling 2026-11-02): the next step is to make
it reproduce. Run the build tier with this test's feature lane beside it, or
`just check gate-order build`, and capture the router's and the executor's
state at the timeout.

## What would close it

The cause found and fixed, and `just check gate-order build` showing the same
verdict for `required-features-tests` in both orders on a loaded host.

## Repro attempts (2026-11-07, newslab-118, 48 cores)

Not reproduced. `case_2_one_sample_per_dispatch` ran 8 copies at a time under
gdb, each run SIGINT-ed with a full thread backtrace if it outlived 30 s:

- 320 runs beside a local `just ci gate` (compile and unit stages): 0 hangs;
- 480 runs while that gate was in `check::build`, the stage this issue was
  seen beside: 0 hangs.

(Each pass logged one "FAIL" that was a false positive: a gdb line split
`test result: ok` across two lines.)

**Suspect, not confirmed.** Since issue 2001 a RELIABLE publisher is declared
`Z_CONGESTION_CONTROL_BLOCK`, so `publish_serialized` waits on the transport's
TX mutex with no deadline, i.e. on another thread's socket write. If the
router stalls under load, the publisher thread can hold or wait on that mutex
indefinitely. The main loop's asserts (10 s) would then panic, but unwinding
drops the executor, and closing the session needs the same mutex, so the test
would hang to nextest's 60 s timeout instead of reporting the panic. That fits
a TIMEOUT with no bound of 60 s anywhere in the test, but nothing measured it.

**Watchdog (branch `issue-2352-race-watchdog`).** The test now records its
stage (open, setup, round n, joining the publisher thread, dropping the
executor) and aborts at 45 s, writing the stage to the stderr handle (not
`eprintln!`, which libtest captures and an abort discards). Negative control:
with the limit set to 0 it printed `stuck while opening the executor (session
open to the router)` and aborted; at 45 s both cases pass. So the next
occurrence names where it blocked. If it says "dropping the executor" or
"joining the publisher thread", the suspect above is the lead.

A side finding: running both cases in ONE process (`cargo test`, not nextest)
fails the second `Executor::open` with `Transport(InvalidConfig)`; nextest's
process per test never hits it.

Repro loop used (gdb is needed: `ptrace_scope=1` forbids attaching to a
sibling, so each run starts under gdb):

```bash
gdb -batch -ex 'set pagination off' -ex 'handle SIGPIPE nostop noprint' \
    -ex run -ex 'info threads' -ex 'thread apply all bt' \
    --args target/debug/deps/entity_wake-<hash> \
    'an_arrival_racing_a_drain_is_never_left_undispatched::case_2_one_sample_per_dispatch' \
    --exact --nocapture > run.log 2>&1 &
# at 30 s: kill -INT <the inferior>, and gdb prints every thread's stack
```

## Repro attempts (2026-11-11, 32 cores, `/home` I/O-saturated)

Not reproduced. Conditions were harsher than 2026-11-07: besides this
session's `just check gate-order build`, other sessions' builds kept the
`/home` disk saturated — `/proc/pressure/io` `full` avg10 of 85–95 % for
long stretches, 4–7 GB dirty, and a one-line write to a file beside the
port leases under `build/` taking over 1 s 95 times and up to 21.4 s
(sampled once a second through both runs). The gate-order run took 6717 s
ascending and 3683 s descending.

- solo, 20 runs: 20 passes, about 0.48 s each;
- beside the ascending build-tier run, 8 copies at a time, each run its own
  process (the binary run directly, `--exact`): 600 + 3 000 + 30 000 +
  11 737 + 30 000 = 75 337 runs, **0 failures**, including the 1 147 s
  the `required-features-tests` gate itself was running (it ran this test
  too);
- beside the descending run, the same way: 45 965 runs, **0 failures**.

`just check gate-order build` (one run, both orders):
`build::check::required-features-tests` PASSED in both orders — the
verdict this issue is closed on, but one green pair after one red pair is
not evidence the cause is gone. The run as a whole FAILED on other gates,
all 2 400 s TIMEOUTs under the same I/O load: `build::check::cli-tests` in
both orders, `build::check::test-targets` in the descending one only.

A false lead worth recording: an earlier loop that wrote each run's output
and the router's log (`NROS_TEST_LOGS=1`) to files under `/home` failed 12
of ~3 400 runs — 6 hung past 70 s, 6 failed `the ROS zenoh router failed to
start: Timeout`. One of the hung processes was in `balance_dirty_pages`,
i.e. the test's OWN log writes were being throttled. Moving the run logs to
`/tmp` (another disk) and dropping `NROS_TEST_LOGS` made those failures
disappear (120 702 runs, 0 failures), so they measured the loop, not the
test. Under nextest the test's output is a pipe and the router's is `null`,
so this does not explain the original TIMEOUT.

What the test writes on the `/home` disk under nextest is only its port
lease (`build/nros-tests/port-leases/*.lock`, a pid). The router's stderr
is piped and only read when it exits, but `rmw_zenohd` at its default level
writes ~300 bytes per run, far below a pipe's 64 KiB, so a router blocked on
its own stderr is not a candidate either. A lease write stalled for tens
of seconds on a saturated disk is the one harness stall these measurements
allow, and it is unconfirmed: none of the 120 702 runs hit it.

**Diagnostics strengthened** (this branch): the watchdog now also covers the
router start and the domain claim (stages `starting the test's router` and
`claiming a ROS domain`, so a harness stall is named rather than reported as
nextest's bare TIMEOUT), and before aborting it prints every thread's stack:
the process allows any tracer (`PR_SET_PTRACER`) and runs `gdb -p <self>`
under a 10 s deadline (`nros_tests::process::deadline_command`). Negative
control: with the limit at 1 ms it printed the stage and five threads'
stacks (zenoh-pico's read task in `recv`, the test thread, the watchdog in
`poll` on gdb) and aborted with SIGABRT. So the next occurrence brings its
own backtraces.

**Side finding explained, not a bug.** Both cases in one process
(`cargo test`) fail the second `Executor::open` with
`Transport(InvalidConfig)` only because libtest runs them CONCURRENTLY and
the zenoh session pool is `ZPICO_MAX_SESSIONS = 1` (`ZpicoError::Full` maps
to `InvalidConfig`, issue 0465). With `--test-threads=1` both pass in one
process (1.09 s), so the first session's slot IS returned on drop; it is
unrelated to this hang.
