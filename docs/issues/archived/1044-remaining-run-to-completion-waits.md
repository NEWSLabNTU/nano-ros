---
id: 1044
title: "The run-to-completion wait class is not finished: an eighth site, four
  bridge callers with an unstated horizon, and an assertion that counts
  re-opens instead of rating them"
status: resolved
type: bug
area: testing
severity: medium
found: 2026-09-04
related: [issue-1013, issue-1026, issue-0670]
---

## Why this exists

Issues 1013 and 1026 are resolved and archived: the pubsub cell no longer kills
its talker, and six waits aimed at free-running nodes now wait on a condition.
Both recorded work they deliberately did not take, and that work would be lost
in an archived file. It is collected here so it has an owner.

## What is left

* **`tests/action_multigoal.rs:76` — an EIGHTH site of the class**, which issue
  1026's original table does not list. Found by its own sweep
  (`rg -n 'or_else\(\|_\| .*wait_for_all_output' packages/testing`) after the
  seven were fixed, so the table was a sample and not a census.
* **The four `Ros2DdsProcess::topic_echo*` siblings** still bake
  `timeout --foreground 10` — the same horizon one layer down, unstated, behind
  bridge/xrce callers. 1026 fixed `Ros2Process::topic_echo` (one caller) into
  `topic_echo_for(…, window)` with a named `DEFAULT_ECHO_WINDOW`, and left these
  alone for a stated reason: their callers are files that wave did not own, they
  still drain to completion so the buffered-until-exit behaviour is load-bearing
  for them, and adding `PYTHONUNBUFFERED=1` would change delivery timing under
  tests nobody there could run. All three reasons are about who was holding the
  file, not about the horizon being right.
* **`SERVICE_CALL_FAILED_MARKER` is a file-local `const` in `services.rs`**, not
  a `nros_tests::output` constant. CLAUDE.md's rule is that test greps use
  `nros_tests::output::*` and never literal strings; every Rust group copy of
  the client prints the same wording and the C/C++ copies print a different one
  (`Service call failed with error %d`), so it wants two constants there.
* **`MAX_ROUTER_SESSIONS` is a count, not a rate** (from 1013). A lease in the
  15-30 s band costs only one or two re-opens inside a 60 s window and can sit
  under the limit, so `assert_no_session_churn` passes on a configuration it
  exists to reject. Closing it means asserting the INTERVAL between opens, or a
  longer window.

## The trap 1026 recorded, worth re-reading before touching any of these

Folding the wait's own error text into the collected output makes the assertion
match the COMPLAINT and pass silently — issue 0670's shape. Keep the diagnostic
and the asserted string on separate channels.

## Acceptance

Each of the four items is either fixed or has a stated bound at its call site,
in the sense 1026 used: a wait that cannot fail is a defect, and a wait whose
horizon is deliberate is fine as long as the deliberation is written down.

## Fixed 2026-09-04 — three of four, and the fourth is stated rather than pretended

### `action_multigoal.rs:76` — the defect was the FAILURE path, not the wait

The wait's SHAPE was right: this client sends six goals, prints one summary and
exits, so run-to-completion is correct — unlike issue 1026's six sites, it is not
aimed at a free-running node. What was wrong is what happened when it did not
find the line. The old spelling

    .wait_for_output_pattern(SUMMARY, 60s)
    .or_else(|_| client.wait_for_all_output(2s))
    .unwrap_or_default()

put the whole transcript inside the strict call's ERROR MESSAGE, `or_else`
discarded it and re-read a client that had already exited, and
`unwrap_or_default()` turned the second failure into `""`. So the panic whose
entire job is to show what the client printed reported nothing. Issue 0471's
shape: the path carrying the evidence was not the path that reported.

**A/B'd, not argued.** Blinding both the wait pattern and the assertion (so the
run takes the path a hung client would):

| spelling | what the panic printed |
| --- | --- |
| old | `multi-goal client printed no summary line. Output:` then **nothing** |
| `collect_until` | the same line, then the full nine-line transcript |

An earlier attempt at this mutation was WRONG and is worth recording: blinding
only the assertion leaves both spellings identical, because the client exits and
both readers return everything on exit. Only blinding the wait as well reaches
the path that differs. A mutation that does not reach the changed code proves
nothing, and it looked like it had.

Its readiness check is fixed too: `... .is_err() && !server.is_running()`
tolerated "no banner, still running", i.e. a HUNG server — after which the client
fires six goals into nothing and the cell reports a wrong summary instead of an
unready server. `wait_for_output_pattern` does not kill on timeout, so the
banner's absence is an independent fact and is now required on its own, with
`is_running` kept only to say which of the two failures happened.

### The `topic_echo*` siblings — the horizon is named

`DEFAULT_ECHO_WINDOW` now spells all four echo sites, and a new
`DEFAULT_PUB_WINDOW` spells the three streaming `topic pub` siblings, which are
the same class one role over: a publisher's `timeout --foreground` is also its
lifetime, so a subscriber-side wait longer than it is waiting on a peer that has
gone. Values are unchanged, so no test's timing moves — this is the "stated
bound" half of the acceptance, not a rewrite.

Each site now says the two things a caller needs: that the window is the peer's
whole LIFETIME, and that without `PYTHONUNBUFFERED=1` the timeout is also the
FLUSH — which is why the `Ros2DdsProcess` family does not take the window as a
parameter, since a caller could otherwise ask for a wait that can never produce
output.

The one-shot `ros2 node list` / `topic info` timeouts are deliberately left: a
command budget on a query that terminates is not this class.

### `SERVICE_CALL_FAILED_MARKER` — promoted, and its twin filed beside it

Now in `nros_tests::output`, with `SERVICE_CALL_FAILED_MARKER_C` for the C/C++
wording (`"Service call failed with error %d"`, so only the prefix is stable).
Both verified against all nine example copies. No test greps the C one yet; it is
there so the next one does not invent a third spelling.

### `MAX_ROUTER_SESSIONS` — NOT closed, and the reason is measured

The count cannot be turned into a rate at this window, and the arithmetic is now
in the constant's doc. A lease `L < 30 s` re-dials every `2L`, so at `W = 60` any
broken lease produces two re-opens — one per node — and 4 sessions against a
limit of 3. The band IS covered on paper. What is not covered is start SKEW: the
listener launches after the talker's readiness banner, so for a lease near 30 s
one node's single lapse can land past the window and leave 3 sessions, which
passes.

Two closes, neither taken:

* **a longer window** — at 120 s every lease under 30 s gives at least two
  re-opens per node and skew cannot hide one. It doubles the cell.
* **counting per NODE** — which would make skew irrelevant. **Unavailable, and
  this is measured rather than assumed:** the client zid is regenerated on every
  session open (`zpico.c`'s `zpico_next_session_zid_counter()` mixes a monotonic
  counter and the clock into it), so the router log cannot tell one node
  re-dialling twice from two nodes dialling once — grouping by zid would read as
  more NODES, not more sessions.

The remaining false-pass risk is filed as **issue 1056** rather than left in a
doc comment: a test that can pass on a build it exists to reject needs an owner.

What did land is the evidence: `session_open_spacing()` prints the gaps between
consecutive opens on both the pass and fail paths, because the SPACING separates
what the count cannot — a lapse repeats every `2 x lease`, a drop happens once.
It is a diagnostic and never an assertion, since it parses third-party log text
(RFC-0075), so a parse miss costs information and not a verdict. Verified on a
real run: `router sessions opened: 2 (max 3); gaps between opens: 0.0s`.

## Verified

* `just check fast` — 197 gates OK.
* `action_multigoal` + `services` — 6 / 6.
* `rtos_e2e` NuttX C++ pubsub — PASS, with the new spacing line.
