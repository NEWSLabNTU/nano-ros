---
id: 799
title: "`nros_time_from_nanoseconds` truncates and takes an absolute value, so
  negative times encode wrong — −0.5 s round-trips as +0.5 s, and time
  arithmetic that crosses zero is silently incorrect"
status: open
type: bug
area: api, core
related: [rfc-0073, phase-379, issue-0789]
---

## Problem

`packages/api/nros-c/src/clock.rs:257`:

```rust
pub extern "C" fn nros_time_from_nanoseconds(nanoseconds: i64) -> nros_time_t {
    let sec = (nanoseconds / NANOS_PER_SEC as i64) as i32;      // truncates toward zero
    let nanosec = (nanoseconds.unsigned_abs() % NANOS_PER_SEC) as u32;  // absolute value
    nros_time_t { sec, nanosec }
}
```

Both halves are wrong for negative inputs, and they are wrong independently.

ROS 2 states the convention in `builtin_interfaces/msg/Time.msg` itself:

> The nanoseconds component, valid in the range [0, 1e9), to be added to the
> seconds component.
> The time -1.7 seconds is represented as {sec: -2, nanosec: 3e8}

That is **floor** division with a **non-negative** remainder. Ours truncates
toward zero and then takes `unsigned_abs`, so:

| input | correct | ours | decodes back as |
| --- | --- | --- | --- |
| −0.5 s | `{sec:-1, nanosec:5e8}` | `{sec:0, nanosec:5e8}` | **+0.5 s** — sign lost |
| −1.7 s | `{sec:-2, nanosec:3e8}` | `{sec:-1, nanosec:7e8}` | **−0.3 s** |

`nros_time_to_nanoseconds` (line 266) decodes with `sec * 1e9 + nanosec`, which
is the correct inverse of the correct encoding — so the round trip exposes the
encoder's error rather than cancelling it.

## Blast radius

`nros_time_add` (line 276) and `nros_time_sub` (line 286) both compute in `i64`
and hand the result to `nros_time_from_nanoseconds`. So **any time arithmetic
whose result lands in (−1 s, 0) comes back positive**, and anything below −1 s
comes back with the wrong sub-second part.

`nros_duration_t` shares the encoding, and a negative duration is not exotic —
"how far ahead of the deadline are we" is negative half the time.

A `nros_time_t` written into a message header is a `builtin_interfaces/Time` on
the wire, so a wrong encoding is an interop bug and not only an internal one.

## How it was found

Building the C++ `Time`/`Duration` surface for issue 0789 on top of this C API.
`Time::to_msg` delegates to `nros_time_from_nanoseconds`; `Duration::to_msg`
deliberately does **not**, and open-codes the floor/remainder split instead,
because delegating would have inherited this bug for signed spans. That
asymmetry inside our own new header is the tell — it is documented at the site
and should be removed once this is fixed.

Phase 379's `timer` stage never caught it: the correlator compares names and
argument shapes, not behaviour. `nros_time_from_nanoseconds` has the right name
and the right signature.

## Fix

```rust
let sec = nanoseconds.div_euclid(NANOS_PER_SEC as i64) as i32;
let nanosec = nanoseconds.rem_euclid(NANOS_PER_SEC as i64) as u32;
```

`div_euclid`/`rem_euclid` are exactly floor-with-non-negative-remainder, and
`rem_euclid` on a positive divisor is guaranteed in `[0, 1e9)`, which is the
range `Time.msg` requires.

Worth adding at the same time:

* a round-trip property test over negative, zero, sub-second and multi-second
  values — the existing tests only cover positives, which is why this survived;
* `nros_duration_from_nanoseconds`, which does not exist. Its absence is why the
  C++ `Duration::to_msg` open-codes the split;
* a check on `sec`'s `as i32` narrowing, which silently wraps beyond ±68 years
  of nanoseconds.
