---
id: 995
title: "The build tier is red on three gates, and has been for days — nothing
  runs it, so nothing said so"
status: open
type: bug
area: ci
severity: high
found: 2026-09-02
related: [issue-0993, issue-0981, issue-0952]
---

## Symptom

`just check build` fails three of its 21 gates in CI's container:

```
===== FAIL (borrowed-e2e,   rc=1,  37827ms) =====
===== FAIL (sched-dim-arms, rc=1,    476ms) =====
===== FAIL (source-gates,   rc=1, 496481ms) =====
check-build (parallel): 3 of 21 gate(s) FAILED
```

## It is not new, and that is the point

The scheduled `gate` runs were ALREADY red on `borrowed-e2e`:

```
33582399779  failure  2026-09-02T02:13:11Z   error: recipe `borrowed-e2e` failed with exit code 1
33461214046  failure  2026-09-01T02:05:17Z
```

Two consecutive nightlies, at least two days, and nobody noticed — because no
pull request and no merge group runs this lane, which is exactly what issue 0993
is about. 0993 predicted an ungated lane accumulates rot; this is the rot, found
by trying to gate it.

The two other failures were invisible for the same reason and may be older; no
attempt has been made to date them, because the nightly stops at the first
failing gate in a serial list and only the parallel runner (0993) reports all
three at once.

## Known so far

* **`sched-dim-arms`** — compiles a probe against the FreeRTOS submodule
  sources: `freertos_run_tiers.c:20:10: fatal error: FreeRTOS.h: No such file or
  directory`. The container does not have them where the gate looks. Whether
  that is a missing submodule init in the job or a wrong path in the gate is NOT
  established.
* **`borrowed-e2e`** — builds `nros-c` for platform-posix and reads the
  generated config header. Fails after ~38 s. The local failure mode in a
  pristine worktree was `nros-c config header missing at
  <root>/target/nros-c-generated/nros/nros_config_generated.h`; whether CI's is
  the same has not been checked.
* **`source-gates`** — fails after **496 s**, via `_nextest-tolerant`. Not
  diagnosed at all.

None of the three is diagnosed here. This is a filing, not an investigation:
the point is that three gates are red, the tree does not know it, and the next
person to touch this lane should start from that rather than from a green
assumption.

## Why it matters beyond the three

A lane that is already red cannot start gating pull requests — issue 0993's
attempt to do so is blocked on this, and was reverted for it (plus a cost
finding recorded there). So this is the thing standing between the build tier
and the merge path.

## Direction

1. Diagnose the three separately; they look unrelated.
2. `source-gates` at 496 s is also the lane's cost pole. Whatever fixes it
   should say what it costs, because that number decides whether the lane can
   ever gate a pull request.
3. Only then revisit gating (issue 0993).

## Acceptance

* `just check build` is green in CI's container.
* Each of the three has a stated cause, not just a passing run.
