---
id: 1161
title: "The skip budget forbids a missing FIXTURE and permits a missing CAPABILITY, so `check-required-features-tests` reports pass having run 7 of 20"
status: resolved
type: bug
area: testing, ci
severity: medium
found: 2026-09-06
resolved: 2026-09-08
related: [0584, 0673, 1168]
---

> **Filed premise was wrong, corrected 2026-09-06.** This was filed as "a
> `skip!` is a failure in one lane and a skip in another, and `just ci gate`
> runs both", from reading a raw nextest `Summary` line as the lane's verdict.
> It is not the verdict, and CLAUDE.md says so in as many words: *"Bare
> `cargo nextest` counts `nros_tests::skip!` panics as FAILURES ... `Real
> failures: N` from the junit rewrite counts what the RUN saw."* The lane does
> not use a bare run — it goes through `_nextest-tolerant`, which rewrote the
> 13 and printed `All failures were [SKIPPED] preconditions — treating as
> pass`. The red I attributed to this was issue 1168's timer flake, four steps
> later. What survives is a different and real problem, below.

# A lane that ran 7 of its 20 tests, and said pass

```
check-skip-budget: 7 ran, 0 deselected (out of lane), 13 skipped for an unmet
                   precondition — capability=13
All failures were [SKIPPED] preconditions — treating as pass.
```

Thirteen of twenty tests never executed, on a host with no
`ros-<distro>-rmw-zenoh-cpp`, and `check-required-features-tests` is green. That
is the shape issue 0584 exists to prevent — "a lane greens over a coverage
hole" — and the guard was built and then applied to one of the two classes.

## The rule is already written; it names fixtures only

`scripts/test/check-skip-budget.py` asserts exactly two properties, and the
second is:

> **No skip whose reason is a missing fixture.** Since 0584 part 2 an absent
> in-lane fixture is a hard failure, not a skip.

So a missing FIXTURE fails. A missing CAPABILITY — here `rmw_zenoh_cpp/rmw_zenohd`,
resolved through `AMENT_PREFIX_PATH` — is counted, printed, and tolerated. The
difference is not principled: both are "the thing this test needs is not here",
and both make the run do less than it claims. It is historical, because 0673
introduced the capability tolerance to stop tier 1 going red for an environment
fact, before 0584 established that an unmet precondition should fail.

The cost is the one already recorded for uniformly-red lanes, in the other
direction: **these 13 have no signal capacity.** A regression in any of them
looks exactly like today's skip, and they are worth signal — issues
0652/0612/0667 found four targets broken and one capability non-functional the
first time this lane's targets actually ran.

## Fixed — a capability skip is a FAILURE unless the lane declared it

`check-skip-budget.py` gains the rule 0584 gave fixtures, one class over: a
`capability` skip whose reason matches no line in
`.config/capability-skip-baseline.txt` fails the run and names what was missing.

A RATCHET, not an allowlist. Each line states what is absent, why a host may
legitimately lack it, and what would retire it. The set may shrink freely; it
cannot grow without someone writing that paragraph. A MISSING baseline file
declares nothing rather than allowing everything — the direction that keeps a
ratchet ratcheting when someone deletes it, and it is self-tested.

Two entries, and neither is the one this issue was filed about:

* **`rmw_zenoh_cpp/rmw_zenohd`** — the router ships as a ROS package that is not
  part of a base install, and RFC-0075 resolves it through `AMENT_PREFIX_PATH`
  rather than PATH, so "installed but not sourced" correctly reads as absent
  (issue 0774: an unsourced router is one the loader pairs with the wrong
  `libzenohc.so`). Retires when the lanes needing it provision it.
* **`ZPICO_MAX_SESSIONS=1`** — the default single-session shim, which is the
  right default for a shipped target. The multi-session paths have their own
  lane. Retires when the cell moves to a coordinate the selector can express, so
  it deselects as `lane` instead.

## What the rule caught on its first run, and it was not what I expected

The lane this issue is named after now runs **20 of 20**: the router got
installed on this host between the filing and the fix, so the 13 skips are
gone and the rule never fires there. What it did fire on was `test-unit`, twice:

1. **A MISCLASSIFICATION.** "default locator port 7447 … already in use" was
   reported as `capability`, because a bare `skip!` defaults to that class and
   the site never said otherwise. A bound port is a `resource` — a runtime
   prerequisite a rerun or a quieter host resolves — and the two have opposite
   remedies. Mislabelled, it would have demanded a baseline entry for a
   condition nobody can provision away. Now `skip_class!(resource, …)`, and the
   breakdown reads `capability=1 resource=1` where it read `capability=2`.
2. **The `ZPICO_MAX_SESSIONS` cell**, which CLAUDE.md already records as
   "skipped on every host in every tier" — visible in a budget line nobody was
   required to read, and now declared with the lane that does run it.

That first one is the argument for the rule. The class was wrong by DEFAULT, in
a lane that was green, and nothing would have said so.

## Verified

Both directions, against crafted junit rather than by waiting for a host to lack
something:

```
declared capability   -> rc 0
undeclared capability -> rc 1, naming the reason and the first test
```

Plus four self-test cases on the declaration itself (declared allowed,
undeclared refused, empty declaration refuses everything, missing file declares
nothing).

```
just test-unit                    1214 ran, capability=1 resource=1, rc 0
just check required-features-tests PASS (20 of 20 on this host)
just check fast                   275 ran, 1 ledger skip, 0 failed
```

## Measured

| | |
| --- | --- |
| host | no `ros-humble-rmw-zenoh-cpp` installed (`dpkg -l` = 0 matches, `/opt/ros/humble/lib/rmw_zenoh_cpp` absent) |
| lane | `check-required-features-tests`, first step |
| ran | 7 |
| skipped `capability` | 13 |
| verdict | pass |

The 13 are in `trigger_conditions`, `wake_latency`, `component_runtime`,
`component_dispatch`, `component_param` and `signal_fd_wake`, all panicking at
`fixtures/zenohd_router.rs:560`.

## Not in scope here

The `nros-platform-cffi` timer red seen in the same run is issue 1168 — a
compute-budget flake, fixed there — not a skip-accounting problem.
