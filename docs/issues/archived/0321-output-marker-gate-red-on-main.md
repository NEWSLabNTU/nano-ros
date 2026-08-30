---
id: 321
title: "output_marker_gate is RED on main: six inline marker literals in the ros-editions family, and the gate's lane never co-runs with the family it polices"
status: resolved
type: bug
severity: high
area: testing
related: [issue-0157, issue-0164]
---

## Finding (audit 2026-07-28, P1 — reproduced by running the gate)

```
$ cargo nextest run -p nros-tests --test output_marker_gate
ros_editions_zenoh.rs:111: literal `Result of add_two_ints:` — use nros_tests::output::*
ros_editions_e2e_pubsub.rs:61: literal `I heard:` — use nros_tests::output::*
ros_editions_e2e_service.rs:32: literal `Result of add_two_ints:` — use nros_tests::output::*
ros_editions_xrce.rs:82: literal `I heard:` — use nros_tests::output::*
ros_editions_xrce.rs:104: literal `Result of add_two_ints:` — use nros_tests::output::*
test result: FAILED. 0 passed; 1 failed
```

Six offenders (the sixth is `ros_editions_zenoh.rs:87`). `just check fast`
stays green because the gate is a TEST, not a check — so this is red in the
`test-all` half of `just ci`.

## Why it landed unnoticed (the interesting part)

`justfile:1174` deselects the ros-editions binaries from the default sweep
(`env_exclude+=("not binary(~ros_editions)")` — they need docker and a slow
per-edition image). But `output_marker_gate` is a **separate binary that is not
excluded**, and it reads `tests/*.rs` from **disk**.

So the gate polices source files whose own test binaries never compile or run in
`just ci`, while the family's own lane (`just ros_editions ci jazzy`) never runs
the gate. Two lanes, neither covering the other — the literals were invisible
from both sides.

## Secondary: the gate duplicates the SSoT it guards

`packages/testing/nros-tests/tests/output_marker_gate.rs:16-26` hardcodes its own
9-entry `MARKERS` table instead of consuming `src/output.rs`, which declares
~30 markers. It enforces an SSoT using a second copy of the data. The gap is
already live: `xrce_ros2_interop.rs:390` uses `"SUCCEEDED"` / `"Result:"`,
marker-ish strings absent from the gate's table, so they pass unflagged. (This
also made the audit's own pre-scoped grep undercount: 3 of the 9 markers were
missing from it, hiding 3 of the 6 real violations.)

## Fix

1. Replace the six literals with `nros_tests::output::{LISTENER_LOG_PREFIX,
   SERVICE_RESULT_PREFIX}` / `output::listener_line(42)` /
   `output::service_result_line(5)`.
2. Export `pub const ALL_MARKERS: &[&str]` from `output.rs` and have the gate
   iterate that, so its coverage can never drift below the constant set.
3. Decide where the gate belongs: either co-run it with the family it polices,
   or keep it in the always-on lane and accept that it lints un-run sources —
   but say which, in a comment, so the next person doesn't rediscover the
   two-lane blind spot.

## Resolved (2026-07-28)

**Primary — the six literals.** Replaced with the SSoT constants across the
four ros-editions files:

- `out.contains("I heard: [42]")` →
  `out.contains(&format!("{} [42]", nros_tests::output::LISTENER_LOG_PREFIX))`
- `out.contains("Result of add_two_ints: 5")` →
  `out.contains(&nros_tests::output::service_result_line(5))`

The service form uses the existing `service_result_line(5)` helper rather than
the bare prefix, so the assertion still pins the value, not just the label.

**Secondary — the gate no longer restates the table it guards.** `MARKERS` was
nine inline literals; it now references `output::*`. That mattered in the
direction the issue did not spell out: a marker RENAMED or retired in
`output.rs` used to leave the gate policing a string nothing emits — passing
forever while guarding nothing. Referencing the constants makes that
unrepresentable.

Eight of the nine markers already had constants. The ninth,
`"Goal accepted by server"`, existed only as part of the longer
`ACTION_GOAL_ACCEPTED_MARKER` (`"…, waiting for result"`). Substituting the
longer constant would have WEAKENED the gate — a test spelling the bare prefix
would stop being flagged — so `ACTION_GOAL_ACCEPTED_PREFIX` was added and the
relationship documented on both.

### Receipts

- `cargo nextest run -p nros-tests --test output_marker_gate` → PASS (was FAIL
  with six offenders).
- **Mutation-checked:** a `const _P: &str = "Goal accepted by server";` probe
  added to `ros_editions_zenoh.rs` makes the gate FAIL — so the rewired table
  still bites, and specifically through the newly added prefix constant.
  Restored clean.
- All four ros-editions binaries compile (`cargo test --test <t> --no-run`).
  This matters here: `justfile:1174` deselects them from the default sweep, so
  a compile break in exactly these files is what the lane cannot see — the
  issue's own point, applied to its fix.
- `just check` green.

### Not addressed

The lane gap itself. `output_marker_gate` still polices sources whose binaries
never run in `just ci`, and `just ros_editions ci <edition>` still does not run
the gate. Fixing the literals removes today's red; the structural hole that let
them land stays open, and is the same shape as issues 0314 and 0337 — a gate
and the thing it guards living in lanes that never co-run.
