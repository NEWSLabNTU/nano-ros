---
id: 447
title: "realtime_tiers native/rust: the 10 ms high tier publishes nothing while the 100 ms low tier runs"
status: open
type: bug
area: runtime
related: [issue-0422, issue-0438, phase-263, rfc-0032]
---

## Symptom

`realtime_tiers_e2e::realtime_tiers`, cell `native/rust`, on freshly built
fixtures:

```
[native rust] high-tier /ctrl counter 0 is not ≥3× the low-tier /telem
counter 4 — the 10 ms tier is not outrunning the 100 ms tier
(phase-263 B2 `nros::main!` run_tiers (RFC-0032 §5); #158 counter proof)
```

## What the numbers mean

The test anchors on the SLOW tier first and that anchor PASSES: `/telem` reaches
5 deliveries, so `telem_max = 4` (the counter is 0-indexed). Only then does it
compare tiers.

`ctrl_max = 0` is not "the high tier is slow" — it is the `unwrap_or(0)` of
"no counter value could be parsed from the `/ctrl` observer at all". So the 10 ms
tier appears to publish **nothing** while the 100 ms tier publishes normally.

The two observers are symmetric in setup (`spawn_listener("/ctrl", …)` and
`spawn_listener("/telem", …)` on the same locator), and the drain asymmetry in
the test is deliberate and correct: `telem_out` was partially consumed by the
anchor wait, `ctrl` has no earlier reader, so `ctrl_all` legitimately holds
everything `/ctrl` produced.

## Not yet established

Which of these it is:

1. **The high tier never runs.** `run_tiers` (RFC-0032 §5) spawns per-tier
   execution; if the 10 ms tier is not scheduled, nothing publishes.
2. **The high tier runs but does not publish.** Tier binding via the
   `node_name → sched_context` table could attach the wrong node.
3. **The observer never receives.** A `/ctrl` topic/QoS mismatch would starve
   the observer while `/telem` works.
4. **A parse failure.** `max_int_after(prefix)` finding nothing would also yield
   0 — the assertion just above (`telem_max > 0`) guards that for telem but
   nothing proves the ctrl output was PARSEABLE rather than absent.

Distinguishing (1)-(3) from (4) needs the raw `/ctrl` observer output, which the
failure message does not currently include — it prints the counters, not the
text they came from.

## First step

Print (or dump on failure) the `/ctrl` observer's captured output alongside the
counters. That separates "received nothing" from "received something unparseable"
in one run, and costs nothing when the test passes.

Then run the entry by hand against a router with both observers attached, which
is what isolated the two bugs before this one (#0427, #0429) — the harness hides
whether the publisher or the subscriber is at fault.

## Notes

Distinct from #0438, which is about the same test file: that one is the
`multi-tier` marker the native board never emits (a boot-path assertion). This
is the runtime counter proof, and it fails AFTER the binary is running — the two
have different causes and should not be conflated.

Found triaging #0422 on freshly rebuilt fixtures, after the entry-name fix
(#0411 class) and the shared-probe-dir regression were cleared. Both of those
previously masked this one.
