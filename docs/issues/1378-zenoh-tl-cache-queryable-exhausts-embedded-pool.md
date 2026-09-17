---
id: 1378
title: "on an EMBEDDED zenoh image the action `/status` publisher's
  transient-local CACHE QUERYABLE cannot be declared, so
  `nros_executor_add_action_server` returns -1 and the server never starts"
status: open
type: bug
area: rmw, zephyr, nuttx
severity: high
related: [issue-1341, issue-1361, issue-0460, phase-455]
---

## Why this exists as its own issue

Issue 1341 (the zenoh shim REFUSING `TRANSIENT_LOCAL` on the action `/status`
publisher) is fixed and archived, and issue 1361 (the terminal status sample
never being published) is fixed and archived. This is the finding that was
recorded inside 1341 and outlives it: the publisher is now SERVED, and the
cache queryable it therefore declares is what an embedded image cannot provide.
Archiving 1341 without moving this out would have buried it.

## Symptom, as 1341 recorded it (2026-09-15, not re-measured since)

Nightly **34940586021** (schedule, 2026-09-15T07:13), job **104288439509**
(`nuttx`), cells `test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
and `…lang_3_Lang__Cpp`:

```
nuttx E2E failed — readiness pattern 'Waiting for action goals' not observed.
```

and the server's own transcript says why it never got there:

```
[ERROR] nros: [0.054000] qos: publisher
  '0/fibonacci/_action/status/action_msgs::msg::dds_::GoalStatusArray_/TypeHashNotSupported'
  asked for TRANSIENT_LOCAL and its cache queryable could not be declared (Full…
[nros] examples/qemu-armv7a-nuttx/c/action-server/src/main.c:236
  nros_executor_add_action_server(&app.executor, &app.action_server) -> -1
```

**The asymmetry is measured, not inferred.** In that same job the RUST action
cell passes: `9 tests run: 6 passed, 3 failed` — the three reds are action C,
action C++ and the pubsub C cell that issue 1363 covers. Rust action, and all
three service cells, are green.

## What this points at, and what is not yet measured

A transient-local publisher's cache is a queryable under an `@adv/pub` suffix
(1341 read that off a live router), and on an embedded image the queryable table
is a small static array — `ZPICO_MAX_QUERYABLES` is 8, against which
`[param_services]` (6) and `[lifecycle]` (5) already claim eleven slots before
the app declares anything (issue 0460). That makes pool exhaustion the obvious
reading of `(Full…`, and the next thing to MEASURE rather than a conclusion:
nobody has yet counted the declared queryables in that image.

## A second, smaller defect in the same line

The message is cut mid-word at `(Full…`, so whatever the shim's reason code says
after it never reaches the log. A diagnostic that truncates exactly where the
cause is named costs the reader the one fact they came for.

## What would close it

1. Count the declared queryables in the failing NuttX C image and say whether
   the pool is the cause — a count, not an inference.
2. If it is: the derivation that sizes `ZPICO_MAX_QUERYABLES` must count a
   transient-local publisher's cache queryable on the embedded path too. It
   already does on native (1341's own fix derived 4), which is consistent with
   the Rust cell passing and the C/C++ ones failing, and is the first thing to
   check.
3. Untruncate the diagnostic, whatever the answer to 1 and 2 is.
