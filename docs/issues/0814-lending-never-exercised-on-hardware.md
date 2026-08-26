---
id: 814
title: "The whole zero-copy surface sits behind `feature = \"lending\"`, which only
  a posix test crate ever enables"
status: open
type: test-gap
area: rmw
related: [issue-0812, issue-0813, phase-391]
---

## Problem

`SlotLending` (publish) and `SlotBorrowing` (subscribe) are both
`#[cfg(feature = "lending")]`. The only crate in the tree that turns the
feature on:

```toml
# packages/testing/nros-tests/Cargo.toml:98
nros-rmw-zenoh = { path = "...", features = ["lending", "platform-posix"], optional = true }
```

`platform-posix`. So the zero-copy path is exercised **only on a desktop host**,
against the posix platform, and never on any embedded target — which is the
only place the feature's stated benefit (RAM-tight, copy-count-sensitive)
applies. A scan of the mr_canhubk3/s32k344 image finds one loan-related symbol
and no live path.

## Why it is not merely "untested"

Two defects were found by *reading* the code during a memory-allocation review,
not by any test:

- [issue 0812](0812-publisher-loan-heap-allocates-per-loan.md) — a `Box::new`
  per loan, i.e. a malloc on the zero-copy path
- [issue 0813](0813-zenoh-tx-buf-hardcoded-and-unpriced.md) — a hardcoded 1 KiB
  ceiling that the feature's own use cases exceed

Both are the kind of thing an embedded lane would have surfaced immediately.
Neither is visible from a posix host with a large heap and no size budget.

## What the gap actually is

`RFC-0010` records that loan/borrow are **exclusively raw** — the lent slot is
`len` bytes, because CDR length is not known before encoding, so
`Publisher<M>` has no typed `loan()` dual. That makes the feature's users
precisely the byte-oriented embedded backends (uORB on PX4, and any
POD-struct transport), and those are exactly the targets no lane builds with
`lending` on.

Minimum useful coverage: one embedded lane (qemu tier is enough) building with
`lending` enabled, exercising loan -> commit and try_borrow -> drop.
