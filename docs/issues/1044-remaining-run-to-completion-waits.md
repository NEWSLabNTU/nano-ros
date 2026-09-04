---
id: 1044
title: "The run-to-completion wait class is not finished: an eighth site, four
  bridge callers with an unstated horizon, and an assertion that counts
  re-opens instead of rating them"
status: open
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
