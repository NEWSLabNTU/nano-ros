---
id: 813
title: "`ZENOH_TX_BUF` is a bare const, so the loan path's 1 KiB ceiling is neither
  tunable nor visible in the pool inventory"
status: open
type: tech-debt
area: rmw
related: [issue-0815, issue-0812, phase-392]
---

## Problem

```rust
// packages/rmw/zenoh/nros-rmw-zenoh/src/shim/publisher.rs:488
pub const ZENOH_TX_BUF: usize = 1024;
```

Three consequences, none of them documented anywhere a consumer would look:

| consequence | detail |
| --- | --- |
| **hard 1 KiB ceiling** | `try_claim` returns `TransportError::TooLarge` for `len > ZENOH_TX_BUF`. The zero-copy path cannot carry an image, a scan, or any payload the feature is most attractive for |
| **not a knob** | every other pool of this shape is an env/Kconfig knob. This one cannot be raised without editing the crate |
| **not priced** | it is therefore invisible to `scripts/gen-pool-inventory.py`, which is the tool that exists specifically so consumers can find sizing knobs (issue 0739, from issue 0271) |

## Why this is the exact failure issue 0271 recorded

Issue 0271 audited a 256 KB-class image that had been "rightsized" with nine
tuning envs and still carried ~145 KB of defaults, and concluded: *"the durable
fix is not more knobs, it is making the existing ones enumerable."*
`ZENOH_TX_BUF` is the same shape one level worse — not merely unenumerated, but
not a knob at all.

## Scope note

The arena is `LendArena { busy: AtomicBool, buf: UnsafeCell<[u8; ZENOH_TX_BUF]> }`,
allocated **per publisher**. Raising the constant multiplies by the publisher
count, so making it a knob and pricing it in the inventory belong in the same
change — a knob nobody can see the cost of is how 0271 happened.
