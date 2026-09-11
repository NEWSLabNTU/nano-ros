---
id: 1331
title: "`Rate::period()` returns `nros::Duration` and the upstream-shaped compile
  probe still assigns it to `std::chrono::nanoseconds` — `check cpp` is red on
  main, in a lane no merge-gating event runs"
status: resolved
type: bug
area: api, cpp, ci
severity: medium
related: [1040, 1226, 1020, phase-442, phase-417, RFC-0096]
resolved_in: "phase-442 W5 follow-up (option 1)"
---

## What happens

`just check cpp` fails on pristine `origin/main` (measured at `743070b8d`,
zenoh-pico at its recorded pin `dd071b8d`, no pin moved):

```
packages/api/nros-cpp/tests/compile/ros2_api_adoption_stage2.cpp:263:56:
error: conversion from 'nros::Duration' {aka 'rclcpp::Duration'} to non-scalar
       type 'const nanoseconds' {aka 'const std::chrono::nanoseconds'}
  263 |     const std::chrono::nanoseconds period = rate.period();
      |                                             ~~~~~~~~~~~^~
error: recipe `cpp` failed with exit code 1
```

`c8e7cce6f` (`feat(phase-442 W5): NodeOptions and Rate exist on every target`,
2026-09-11 16:38 UTC) changed the accessor:

```diff
-    std::chrono::nanoseconds period() const { return std::chrono::nanoseconds(period_ns_); }
+    ::nros::Duration period() const { return ::nros::Duration::from_nanoseconds(period_ns_); }
```

and the probe that exercises the upstream spelling was not moved with it.

## Why this is a decision, not a typo

Both sides are deliberate, and that is the whole difficulty.

The **return type** is deliberate. `nros.hpp:1192` says so in place: "`nros::Duration`
rather than `std::chrono::nanoseconds` (phase-442 W5): a return type that exists
only where `<chrono>` does would keep the whole class hostage to the toolchain,
which is what this work item removed." `duration.hpp:16` records the same
constraint one layer down — `to_chrono` is absent because `<chrono>` is not
available on the freestanding targets. W5's stated goal was that `Rate` exist on
**every** target.

The **probe** is also deliberate, and is the thing reporting the breakage.
`ros2_api_adoption_stage2.cpp` is a POSITIVE compile probe whose header says
"Everything in this TU must COMPILE", and `:261` is headed `W2.d: the rate loop,
written the upstream way`. Upstream `rclcpp::Rate::period()` returns
`std::chrono::nanoseconds`, so that line is the assertion that a ported rclcpp
rate loop still compiles unchanged. It now does not.

So rewriting `:263` to `auto` would delete the measurement rather than answer it:
the probe would go green while ported upstream code kept failing at the same
line in a user's tree. What changed is the **parity contract**, and the ledger
has not been told.

## What the ledger says today

`docs/reference/api-parity-ledger/timer.json` carries `cpp:Rate::period` as

```json
{"verdict": "divergence",
 "why": "phase-417 W2.d — `rclcpp::Rate`/`WallRate` as a FORWARDER onto
         `nros::spin(remaining_ms, poll_ms)`, which DRIVES the executor. …"}
```

That reason is about the *sleep* shape — our `Rate` drives the executor instead
of blocking the thread. It says nothing about the return type, so the row reads
as "same signature, different behaviour" when the signature is now different
too. `cpp:Rate::Rate` has the same `why` and did gain a `nros::Duration`
overload in the same commit.

## Why it reached main, and why it stayed

`cpp` is in `build-serial` (`just/check.just:235`), i.e. `check-build`, which
runs on `schedule` / `workflow_dispatch` only. No merge-gating event runs it, so
nothing between the commit and its merge asked the question, and `gh run list
--branch main` shows only the two known environmental reds. This is issue 1040's
class (a gate that reports nothing accumulates reds) and issue 1226's shape (the
lane a contributor is told to run before pushing is not the lane CI runs).

Related measurement gap: issue 1020 — the parity extractor does not read
`rclcpp_compat.hpp` at all, so no automated check compares our `rclcpp::`
surface against upstream's signatures. The probe is the only instrument that
sees this, which is why it must not be edited into agreement.

## Options, for whoever owns W5

1. **Record the divergence and move the probe to our shape.** Update
   `cpp:Rate::period`'s `why` to state the return-type change and its reason
   (freestanding `<chrono>`), then write `:263` as
   `const nros::Duration period = rate.period();` — or keep a chrono line via
   `std::chrono::nanoseconds(rate.period().nanoseconds())`, which documents the
   adapter a porter must write. Cheapest, and honest, provided the ledger says so.
2. **Restore a chrono-returning accessor for hosted builds only** (`#if
   NROS_CPP_STD` / `__has_include(<chrono>)`, per the both-probes rule in issue
   1240), keeping `nros::Duration` on freestanding. Keeps drop-in ports
   compiling, at the cost of a surface that differs by target — which is exactly
   what W5 set out to remove.
3. **Leave `period()` off the compat surface** and let ported code read the
   period from the value it constructed `Rate` with.

## What would close this

* `just check cpp` green on `main`.
* `cpp:Rate::period`'s ledger row describing the return type as it now is, so
  `check-api-parity` and a reader agree.
* A note in the probe naming the chosen shape, so the next W-item does not
  rediscover this by breaking the lane.
* Optional but the real repair: `cpp` reaching a merge-gating event, or a
  cheaper subset of it doing so — otherwise the next signature change lands the
  same way.

## RESOLVED 2026-09-12 — option 1, as filed

`just check cpp` is green on this change.

**The probe moved to our shape**, not around it:

```cpp
const nros::Duration period = rate.period();
```

with a comment at the line naming the divergence, the reason (a return type that
exists only where `<chrono>` does would keep the whole class hostage to the
toolchain, which is what W5 removed), and the one-call adapter for a ported file
that must keep a chrono type —
`std::chrono::nanoseconds(rate.period().nanoseconds())`. Written rather than
adapted, so the probe measures our surface.

**Option 2 was refused for the reason the filing gave.** Restoring a
chrono-returning accessor for hosted builds only would re-create exactly the
surface-differs-by-target property W5 deleted; `Rate` used to be ABSENT on
ThreadX, not merely different.

**`cpp:Rate::period`'s ledger row states the return type as it now is**, with
the argument, the porter's edit, and a pointer to the probe. It also records the
one residue: the `std::chrono` CONSTRUCTOR is still behind
`NROS_CPP_HAS_STD_CHRONO`, which is a gate on a METHOD — permitted, since
`sizeof(Rate)` is `int64_t` + `uint64_t` in every configuration — and phase-442
W8 owns retiring it.

## What this does NOT close

The filing's last bullet, and it is the one that matters:

> Optional but the real repair: `cpp` reaching a merge-gating event, or a
> cheaper subset of it doing so — otherwise the next signature change lands the
> same way.

It did land the same way. W5 changed a public return type, every merge-gating
lane stayed green, and the red sat on `main` until someone ran the
`build-serial` lane by hand. That is issue 1226's shape — a gate that WORKS is
not a gate that RUNS — and it is unaddressed here. The cheap subset would be the
per-header parse loop plus the compile-test TUs, which need no fixtures, no SDK
and no cross toolchain; the expensive parts of `check cpp` are the backend
builds around them.
