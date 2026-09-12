# Phase 455 — a saturated zenoh queryable says so, and the rate that follows is measurable

**Status (2026-09-12). W1, W2, W2.b and W3 LANDED; W4 is the remainder and is
BLOCKED by issue 1341.** Closes phase-444 W2's acceptance by a different
route than phase-444 assumed: not by re-running the hardware measurement, but by
giving the failure an observable so it stops needing one. Implements the
observability half of RFC-0089's rule — a failure a caller cannot see is a
contract the caller cannot hold us to — and mirrors phase-444 W1, which landed
the same shape on Cyclone.

## What landed, and the one thing that blocks the rest

| item | state | evidence |
| --- | --- | --- |
| W1 — the counter | LANDED, `022e94157c` | per-(session, queryable) refusal/saturation counters behind `zpico_reply_slot_stats`, one `nros_log` line per TRANSITION; negative control fails with the counter removed |
| W2 — the completion rate | LANDED, `65d7197380` | `completed 3/3` after a 10 s soak; and it says in its own doc comment what the green does NOT cover |
| W2.b — the control W2 lacked | LANDED, `ef8e09bc6c` | a synthetic declined-query source; RED with `1a032a10b` reverted (`held=4 finalised=0`), GREEN with it (`declines=6 held=0`) |
| W3 — the Zenoh action cells | LANDED, `c2584bda6c` | first Zenoh action interop cells in the tree; n2r PASS live, r2n FAIL citing 1341 |
| W4 — the probe on NSOS | **NOT STARTED, BLOCKED** | see below |

**W2's acceptance as this document first wrote it was NOT met, and W2.b is why
it exists.** The rate probe measured `completed == sent` and `refusals == 0`,
then tried to break itself and could not: reverting either leak arm changed no
observable, and so did a ONE-slot table. Issue 1332 has the measurement. The
cause is structural — the leak is fed by queries the callback DECLINES, and on
the native lane nothing sends them: graph discovery has been a liveliness
SUBSCRIBER since phase-381 (a subscriber delivers samples, not queries, so a
peer joining or leaving never reaches another node's queryable callback), and
both action client paths are single-in-flight. So the lane lacked the
population, not the code path, and W2.b built the population.

**Route 1 is measured and negative.** A real `rmw_zenoh_cpp` peer sends our
queryable only the requests it means to send: 4 queries in 110 s across a daemon
restart, a talker, a listener, three rounds of graph introspection and a 60 s
idle soak — zero declines, `payload_len=20` throughout, and a 12-way concurrent
service-call flood answered 12/12 with none either. W3's cells will therefore
NOT be the stronger control 1332 hoped for. The synthetic source is the control.

**W4 is blocked by [issue 1341](../issues/1341-zenoh-action-server-refuses-its-own-status-qos.md).**
`b0ea5a04b` (phase-428 W9) made the zenoh shim refuse `TRANSIENT_LOCAL`; the
action `/status` publisher passes exactly that, so every zenoh action server
exits 1 at node declaration — including the one W2's probe drives. Running the
probe on NSOS cannot begin until that is decided, and the decision is not ours:
granting VOLATILE breaks RxO against a stock `rclcpp` action client, so it
belongs to phase-428. Nothing about W4's design changes; it is waiting.

## Why this phase exists

[Issue 0902](../issues/0902-action-goal-completion-is-variable.md) measured action
goals completing between 20 % and 90 % on one build, with no session expiry, no
crash and no discovery failure. The mechanism was found: zenoh-pico stores a
cloned query in one of `ZPICO_MAX_PENDING_REPLIES` (4) slots BEFORE the user
callback runs, and the shim's two early returns — the empty-payload liveliness
probe and the ring-full drop — returned above the release. Every background probe
permanently consumed a slot. Once four were gone, `reply_seq` was `-1` forever and
every reply failed silently, discarded at four layers; the C++ side printed
"Goal succeeded" for a goal whose result was never sent.

**Both leak arms are fixed** (`1a032a10b`, `b56e3d50a`): every release now goes
through `_zpico_release_reply_slot`. What has not happened is a measurement, and
phase-444 W2 carries it as "the goal completion rate, measured on a freshly built
image over enough runs to separate it from the 20–90 % the issue recorded."

That acceptance has a defect this phase corrects rather than satisfies. Issue 0902
states it itself:

> A 20–90 % spread with no observable cause is worse than a hard failure. It is
> not measurable as a regression gate, and any future change to this path will be
> evaluated against noise wide enough to hide it.

A rate over N goals is an inference. It answers "did results arrive" and never
"did a slot run out", so it cannot separate this defect from a timeout, a slow
peer, a scheduling artefact or the second candidate the issue records (a per-reply
transport failure). Measured 2026-09-11, **nothing in the image can answer the
direct question**: `zpico_get_diag_counters` carries 18 counters and all 18 are
the client-side `zpico_get*` path; none counts a failed slot allocation, and
`query_handler`, `zpico_query_reply` and `_zpico_release_reply_slot` emit no log
line at all. So a probe today can only infer exhaustion from missing results —
which is the noise 0902 is complaining about.

The tree already decided this question twice, in our favour:

* **phase-444 W1, merged**: Cyclone's `service_take_request` stopped folding
  `WOULD_BLOCK` into `taken = false` + `OK`, because "a saturated server is
  indistinguishable from an idle one". This phase is the same sentence with
  zenoh as its subject.
* **zpico itself** already counts what it drops elsewhere:
  `zpico_graph_entry_count(session, uint32_t *out_dropped)` and the request
  ring's own `dropped`. The reply-slot table is the one bounded resource in the
  file with no such counter.

## The design, and the two things it refuses

**Refused: a wire tap.** Issue 0902's evidence came from
`experiments/serial-interop/serial-tap.py`. That path has never existed in this
repository — not moved, not deleted, never committed, and absent from every
checkout on this disk — so the captures its "two free checks" read cannot be
re-read by anyone. A tap tells you what the wire did and never what the code
decided, and it costs a tool that leaves with the machine it was written on.

**Refused: a host-side observer on NSOS.** `native_sim/native/64` puts the
locator on the command line and the router on an ephemeral port, so a TCP relay
between image and router could log `(direction, size, timestamp)` and reproduce
0902's discriminator (a ~15 B `REPLY_FINAL` between the 82 B query and the 78 B
status) with no root and no hardware. It is ~100 lines and it is still a tap. The
counter below subsumes it for the case in hand; the relay is worth building only
if the counter reads zero while goals still fail — the issue's second candidate —
and then it is a scratch tool under `tmp/`, not a lane.

**Taken: the image says it.** A counter, incremented where the allocation fails,
readable by a test in the same process, versioned with the code, true on every
platform rather than only on the one where the wire is reachable.

## Work items

### W1 [rmw-zenoh] — a failed reply-slot allocation is counted, and said once

`query_handler` (`zpico.c:946-961`) computes `reply_seq = -1` when
`stored_query_valid[idx][*]` is full and passes it on with no record. Count that.
Surface it beside the counters that already exist rather than inventing a second
mechanism: either a new `zpico_reply_slot_stats(session, handle, ...)` in the
shape of `zpico_graph_entry_count`, or new entries in `zpico_get_diag_counters`'s
array — the choice is W1's, made against how a test reaches it and what the
`no_std` shim can bind.

Two properties the count must have, both learned from the issue:

* **Exhaustion is DISTINCT from "no query pending".** The whole defect is that
  those two states were the same value.
* **It is reported once, not per spin.** phase-444 W6 landed exactly this
  correction on the Cyclone parameter services: a permanent failure asked six
  times per spin. A `nros_log` line at the transition into exhaustion, not on
  every allocation.

The Rust side must stop discarding it. `send_response` maps every failure to
`TransportError::ServiceReplyFailed` (`shim/service.rs:547`); exhaustion is a
distinct condition and the executor's four swallow sites
(`action_core.rs:695`, `:706-710`, `arena.rs:1925`,
`nros-cpp/src/action.rs:529-532`) are where "Goal succeeded" gets printed over a
goal that never completed. Fixing the whole error channel is NOT in scope; making
this one condition legible is.

**Acceptance.** A unit or integration test drives a queryable past
`ZPICO_MAX_PENDING_REPLIES` and asserts the counter moves and the log line fires
once; it fails on a tree with the counter removed. `ZPICO_MAX_PENDING_REPLIES` is
`#ifndef`-guarded, so a `-D` can make the test cheap — verify that is still true
(issue 0902 Status item 5 corrected an earlier claim that it could not be
overridden).

### W2 [testing] — the completion rate, with a verdict that is not an inference

A probe that sends N goals, awaits each RESULT (not merely acceptance), and
reports `completed`, `sent` and the W1 counter.

The existing `bins/action-client-multigoal` is the scaffold and should be
EXTENDED, not copied: it already loops goals, retries a timed-out acceptance,
clears the in-flight flag, and prints one machine-parseable summary line. What it
does not do is call `get_result` — it counts acceptance verdicts only, which is
exactly blind to 0902's symptom (accepted, then no result). Its consumer
`tests/action_multigoal.rs` asserts `accepted == 4 && rejected == 2` against the
server's `MAX_GOALS` table and must keep passing; the summary line grows fields
rather than changing meaning.

**The probe must reproduce the conditions the issue measured**, or a green run
proves nothing:

* **An idle soak.** The countdown is consumed by elapsed time with a peer
  present, not by goal traffic — 100 s of idling took the original from 8/10 to
  2/5. A probe that sends N goals back to back can pass on a leaky build.
* **A peer that probes.** The `zenohd_unique` fixture is loopback, multicast off,
  ephemeral port, so no third party reaches the queryable — but liveliness
  arrives through the SAME queryable callback as a real request
  (`shim/service.rs:220-227`), and our own second node is a sufficient source.
  The probe brings one up and down rather than assuming traffic appears.

**Acceptance.** `completed == sent` AND the W1 counter is zero, on the native
host lane. The RATE is recorded as evidence, never as the assertion — that is
what makes this a gate rather than a measurement against noise. The test fails on
a tree with either leak arm reverted; both reverts are named in the commit
message as the negative control.

### W3 [testing] — the Zenoh action interop cells that never existed

`interop::CELLS` has two action rows and both are Cyclone. There is no Zenoh
action interop cell at all, so the backend whose reply-slot table this phase is
about has never met a live ROS 2 peer on the action path. The only zenoh action
coverage is `ros_editions_e2e`, deliberately outside the cell model.

Add `native-action-rust-zenoh-n2r` and `-r2n`, mirroring the Cyclone pair. They
belong as cases in the existing `ros2_action_e2e` binary rather than a new file
(AGENTS.md: new runtime tests join a matrix, not a new file), which means
`assert_test_bound` gains the zenoh coordinate and the per-case guard asks for
`rmw_zenoh_cpp` plus a router where the Cyclone cases ask for
`rmw_cyclonedds_cpp`.

**One trap, measured.** That binary is in `host-dds-ros2-interop` (max-threads 3)
and the zenoh cases need the single-threaded router group. nextest applies the
FIRST matching override per setting, and `binary(x)` is a SUBSTRING match — the
`binary(~zephyr)`-class blanket at the bottom of `.config/nextest.toml` has
already claimed three cells this way. A `binary(=ros2_action_e2e) and
test(zenoh)` override must sit ABOVE the broader ones.

**Acceptance.** Both cells run by a FOCUSED recipe, both produce a verdict row in
`.config/interop-verdicts.toml`, and `check-interop-cell-runners` stays green.
A sweep is not a runner: an unmet precondition inside one is a `skip!` that
renders `<skipped>`, which nobody can tell from a pass.

### W4 [zephyr] — the same probe on NSOS

`native_sim/native/64` reaches a host `zenohd` over real host sockets today
(`tests/zephyr.rs`, `Iso::EphemeralZenohd`), and 40 `matrix::CELLS` rows already
live there. Running W2's probe on it costs one cell and answers a question the
native lane cannot: whether the count holds when the image is a Zephyr build with
its own allocator and thread model rather than a host process.

Gated on W2. It is the closest thing to "on target" that needs no hardware, and
it is explicitly NOT a witness for a device — no RTOS network stack is in the
path, a correction `interop.rs:330-335` already had to make once about this
platform.

**Acceptance.** The cell runs by a focused recipe and produces a verdict.

## What this phase does NOT do

* **Re-measure on hardware.** Phase-444 W2 stays open and stays the owner of
  that. The board run becomes CONFIRMATION of a counter that already says the
  answer, rather than the only evidence anyone has — which is the state that made
  0902 unfalsifiable for a month.
* **Fix the error channel.** The four swallow sites and the C++ "Goal succeeded"
  print are recorded in 0902 and stay there; W1 makes ONE condition legible.
* **Build the wire observer.** See "the two things it refuses".
* **Close 0902.** Its acceptance is a rate at or near 10/10 on a fresh image.
  W2 supplies that on the native lane; the issue's own residual-failure branch
  (the second candidate) stays open until someone has a run that shows one.

## Corrections this phase carries

* **Issue 0902 cites a tool that was never in version control.**
  `experiments/serial-interop/serial-tap.py` appears in no commit in full
  history, under no other name, and in no checkout on this disk; the captured
  dumps are likewise absent. The issue's "two free checks, before any hardware"
  read data nobody has. Say so in the issue rather than leaving the next reader
  to look for it.
* **phase-444 W2's acceptance** is amended to name this phase as the route.
