---
id: 1013
title: "`test_rtos_pubsub_e2e` SIGKILLs its talker after ~12 publishes, so the
  cell exercises twelve seconds of a free-running publisher"
status: resolved
type: bug
area: testing
severity: medium
found: 2026-09-03
related: [issue-0877, issue-0906, issue-1005, phase-414]
---

## What happens

`wait_for_output` is a RUN-TO-COMPLETION wait — its own doc-comment says "wait
for QEMU to produce output *and exit*" — and `rtos_e2e.rs:729` aims it at a
free-running 1 Hz publisher with a 15 s window (`:725`). When the window
expires, `qemu.rs:448` `kill_process_group`s the guest.

    listener t=0 -> stabilization_delay 20 s -> talker t=20
    -> SIGKILL t~35 -> verdict t~35

MEASURED, all three languages, every run: the talker emits exactly **12**
publishes and is then killed. The service and action shapes do not do this —
they use `collect_until` and let the long-lived server run the whole window.

## Why it matters

**It silently bounds what the cell can observe.** Anything whose period exceeds
~12 s of session life is invisible to it, and the cell reports PASS regardless.

That is not hypothetical. Issue 0906 fixed `Z_TRANSPORT_LEASE` 10 s -> 60 s
because a 10 s lease against a 30 s router keep-alive expired every session.
Measured while accepting issue 0877: **rebuilding with the OLD 10 s lease still
passes this cell 6 of 6**, because the first lapse is at ~20 s of session life
and the window closes first.

So this cell cannot regression-test 0906, and nothing else does. Issue 1005 is
the other half — the staleness probe cannot see that constant change either, so
it is unprotected in both directions.

## Direction

Not settled; the choice is about what the cell is FOR.

1. **Use the shape the sibling cells use.** `collect_until` with a predicate on
   messages received, letting the publisher run, is what service and action do
   and why they are not subject to this.
2. **Raise the window** past the longest period the cell must be able to see.
   Cheapest, and it needs a stated number rather than a guess — the lease is 60 s,
   so a window under that keeps 0906 invisible.
3. **Leave the window and state the bound.** Legitimate if the cell is only ever
   meant to prove first-delivery, but then something else must cover session
   lifetime, and today nothing does.

Whichever lands, the acceptance is the counterfactual: build with
`Z_TRANSPORT_LEASE_MS = 10_000` and require the cell to FAIL.

## Fixed in the worktree — direction 1, plus a second instrument the counterfactual forced

> Frontmatter deliberately still says `status: open`. The fix and its acceptance
> are below and were run; what has NOT happened is the integration, and closing
> an issue here means three edits in ONE commit — flip the status, `git mv` this
> file to `archived/`, and add its `Recently resolved` row to
> `docs/issues/README.md` (then `python3 scripts/gen-issue-index.py`).
> `check-issue-index` enforces exactly that set, and README.md is the shared
> registry two sessions collide in, so it belongs to whoever integrates.

**Direction 1 (the sibling shape), with direction 2's stated number folded into
it.** `wait_for_output` is gone from the pub/sub cell. Both nodes now run free
and the cell waits on a COUNT of delivered samples
(`RtosProcess::collect_until_count`, backed by a new `QemuProcess::
collect_until_count` over the existing `collect_until_pred`), killing nothing
until the count is in — which is exactly why the service and action cells were
never subject to this.

Direction 3 was not available: nothing else covers session lifetime, which is
the premise it needs.

### The number: 60 samples, and it is derived

Every talker in the matrix runs a 1 Hz timer, so N samples == N seconds of
session life.

* `_zp_unicast_lease_task` arms `next_lease = lease`, and the FIRST expiry only
  consumes the `_received` the handshake set — a session whose peer stays silent
  closes at **2 x lease**. 0906's 10 s lease lapsed at 19.5 s on the wire.
* The peer is `rmw_zenohd`, whose shipped config announces `lease: 60000,
  keep_alive: 2` — an idle router speaks every **30 s**.
* So `L >= 30 s` holds a session forever and `L < 30 s` closes at `2L < 60 s`.

60 samples is the frontier between those two sets: every client lease that
cannot hold a session against the ROS router is inside the window, and no lease
that can hold one is asked to prove more. Cost, measured: 82 s per FreeRTOS cell
(was ~35 s), 62 s per ThreadX-Linux cell.

**The bound it does not cover, stated:** a defect whose first symptom is past
60 s of session life — the shipped 60 s lease's own lapse would be at 120 s.
That wants a soak, not a bigger per-cell budget.

## The counterfactual, and what it changed about this issue

Both halves, FreeRTOS x {rust, c, cpp}, `just build freertos` between them, bake
verified per rebuild (17 freshly written `zenoh_generic_config.h`, all agreeing):

| build | delivery | router sessions | verdict |
| --- | --- | --- | --- |
| `Z_TRANSPORT_LEASE_MS = 60_000` | 60 published / 60 heard | 2 (one per node) | PASS 3/3 |
| `Z_TRANSPORT_LEASE_MS = 10_000` | **60 published / 60 heard** | **5** | FAIL 3/3 |

**The delivery count does not discriminate, and that is a finding, not a
detail.** This issue (and 0906's own measurement of 77 published / 19 heard)
assumed a lapse still costs messages. On this tree it does not: rebuilt with the
10 s lease, delivery is *perfect* — because the defects that turned a lapse into
lost samples (issues 0899, 0924, and the board's `LWIP_NETCONN_FULLDUPLEX`) have
since been fixed. The reopen now completes in ~15 ms, and a 1 Hz publisher
almost never has a sample in flight during one. A delivery assertion would catch
that build a few runs in a thousand.

So no window length, at any cost, makes the delivery assertion fail on the build
this issue names. What the bad build still does — every 2 x lease, exactly as
0906 measured on the wire — is re-handshake:

    18:30:03  New transport opened ... 676725ea…      <- listener
    18:30:23  New transport opened ... 676725ea…      <- again, 20 s later
    18:30:23  New transport opened ... 1d0e5e56…      <- talker
    18:30:43  New transport opened ... 1d0e5e56…
    18:31:03  New transport opened ... 676725ea…

That is visible from this side of the link for free: the router's own log. So
the cell gained a second assertion, `assert_no_session_churn` — **at most 3
transports opened for two nodes** (measured: exactly 2 on a healthy build, on
FreeRTOS and ThreadX-Linux, all languages; the third slot is slack for one
genuine re-open). It is 0906's stated acceptance ("ONE session for at least five
lease periods, proven by a capture showing no second handshake"), automated from
the router log instead of a packet capture, and it is the half that fails on the
bad build.

Two notes on that instrument:

* The marker is `New transport opened`, not `Accepted TCP connection`. A healthy
  FreeRTOS pair produces THREE accepts and two transports — the first dial is
  abandoned before the handshake. Sessions are the thing being asserted anyway.
* The marker is third-party text (RFC-0075: the router is whatever ROS ships), so
  "fewer than two of these in the log" is a FAILURE naming the constant to fix,
  never zero reconnects. A rename upstream must not turn this into a cell that
  silently stopped checking.

## Verified on

* FreeRTOS (QEMU mps2-an385) rust / c / cpp — PASS 3/3 on 60_000, FAIL 3/3 on
  10_000, both halves re-run against the final code.
* ThreadX-Linux rust / c / cpp — PASS 3/3, 60/60, 2 sessions, 62 s per cell.
  This is also the only coverage of `collect_until_count`'s `ManagedProcess`
  arm.
* NuttX c / cpp — PASS, 60/60, 2 sessions, 81-84 s per cell: a cold arm-virt
  boot does fit `pubsub_window`, and its bring-up does not re-dial.
* NuttX **rust** — could not be run, for a reason that predates this work and
  has nothing to do with it (see below).
* ThreadX-RISCV64 — not run; its fixtures were not built here. The bar is
  protocol-level rather than platform-level, so it should hold, but that is an
  inference, not a measurement.

## Found on the way, unrelated, NOT fixed here

The NuttX **Rust** rtos_e2e cells cannot resolve their fixtures on a machine
using the phase-340 shared cargo dirs. `binaries/nuttx.rs::build_rust_example`
picks the 177.8.c carve-out profile with a LEAF-path probe —

    example_dir.join("target/armv7a-nuttx-eabihf/nros-minsizerel/<bin>").exists()

— but the builder writes into `build/cargo-fixtures/nuttx-<hash>/...` and the
leaf has no `target/` at all, so that probe is always false. The lane then falls
back to the ambient profile and reports

    Test fixture binary not prebuilt: .../nros-relwithdebinfo/talker

naming a profile nothing builds, while `.../nros-minsizerel/talker` sits there
freshly built. The `eprintln!` that explains the fallback is swallowed by
nextest failure-output settings (issue 0982's shape), so only the misleading
line survives. This is #393's rule — move the test-side locator in the SAME
commit as the build — with the probe left behind: `require_prebuilt_binary_fresh`
was migrated to the shared dir, the `.exists()` above it was not. Reproduced
after a clean `just nuttx build-fixtures-arm`, so it is not a half-built tree.
It belongs to whoever owns `fixtures/binaries/`.

## Still open

* **`MAX_ROUTER_SESSIONS` is a count, not a rate.** A lease in the 15-30 s band
  costs only 1-2 re-opens inside a 60 s window and can sit under the limit.
  Closing that means asserting the INTERVAL between opens, or a longer window.
* Issue 1005 is still the other half: the staleness probe does not watch
  `Z_TRANSPORT_LEASE_MS`, so a leaf can carry an old bake with a FRESH verdict.
  Every rebuild in this acceptance was verified by reading the generated header,
  not by trusting the probe.

## Not covered — swept, and it is a class

`wait_for_output` and its five siblings (`ManagedProcess::wait_for_output` /
`wait_for_all_output`, `Ros2Process`, `Ros2DdsProcess`, `ZephyrProcess`, and
`RosPeer::wait_for_output` in `src/ros_env.rs`, which carries no doc warning)
are all run-to-completion-then-kill. Aimed at a free-running node, every one of
them turns its timeout into that node's lifetime. Sites found, none fixed here:

* `services.rs:212` — the worst: its primary wait greps a string no producer
  prints, so the 2 s fallback always runs, and the assertion's third disjunct is
  `!client.is_running()`, which the fallback's own kill makes true. That test
  **cannot fail**, on any build.
* `services.rs:101`, `interop_e2e.rs:483`, `native_api.rs:523` — window is the
  SUT's whole life AND carries the assertion.
* `ros_editions_e2e.rs:190/209/237`, `zephyr.rs:886/977/1681` — blind-collect at
  a `spin=forever` node, asserting only the first event; two of them document
  "the wait always runs the full duration" without drawing the conclusion.
* `native_async_roundtrip_e2e.rs:99` — asserts a MID-RUN marker, so a client that
  hangs after goal acceptance is killed at 40 s and reported PASS.
* `Ros2Process::topic_echo`'s baked `timeout --foreground 10` is the same 10 s
  horizon one layer down, behind four bridge/interop sites.
* `ros2.rs:673` `collect_ros2_output` — dead wrapper, no callers.

Each wants the same treatment this cell got: name what the producer must do,
wait for THAT, and kill only afterwards.
