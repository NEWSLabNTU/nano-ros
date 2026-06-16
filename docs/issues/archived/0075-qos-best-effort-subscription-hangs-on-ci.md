---
id: 75
title: qos_overrides best_effort test fails on CI — test-harness output-consume race (NOT a runtime hang)
status: resolved
type: bug
area: testing
related: [issue-0057, phase-211]
resolved_in: 5e16d1b31
---

## Resolution

`qos_overrides_runtime_delivery::qos_override_best_effort_honored_and_delivers` was
the last host-integration real failure after #57 (OOM) + #71 (multi-std). It
presented as the listener "not becoming ready" on CI while passing locally in ~2 s —
**but it was never a runtime hang.** It is a **test-harness output-consumption bug**.

`ManagedProcess::wait_for_output_pattern` returns its **entire accumulated read
buffer** on match (and those bytes are consumed from the pipe). The test used **two
sequential** waits on the listener:

1. wait for `qos effective: role=Subscription reliability=BestEffort`
2. wait for `Waiting for`

The listener prints `qos effective` → `subscription created` → `Waiting for` in quick
succession. When a single `read()` pulls all three lines into wait (1)'s buffer
(which the test **discards**), wait (2) never sees the already-consumed `Waiting for`
→ `Timeout`. On the CI runner the lines coalesce into one read **deterministically**
(hence the consistent CI failure); locally the reads happen to split, so wait (2)
catches it — that is the entire "CI-only, passes locally" mystery.

**Reproduced locally** with a minimal model (producer emits the 3 lines in one
write): the old two-wait pattern FAILS, a single-wait-plus-assert PASSES.

**Fix:** replace the two sequential waits with **one** `wait_for_output_pattern(
"Waiting for", 12s)` and assert the earlier `qos effective … BestEffort` line is in
that single returned buffer (it precedes `Waiting for`, so it is always present). No
data loss across calls. `qos_overrides_runtime_delivery` passes locally; the
host-integration arc is **11 → 4 → 1 → 0**.

The earlier 4 s → 12 s wait widen (`f9d01feba`) was a wrong slowness guess and is
now subsumed by the single-wait rewrite.

**Latent harness note:** any test doing back-to-back `wait_for_output_pattern` calls
on the same process can lose output the same way (the first call may consume past its
match). Out of scope here; worth a harness hardening pass (e.g. retain unmatched
tail) if it recurs.
