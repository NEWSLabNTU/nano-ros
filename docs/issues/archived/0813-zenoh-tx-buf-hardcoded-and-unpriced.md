---
id: 813
title: "`ZENOH_TX_BUF` is a bare const, so the loan path's 1 KiB ceiling is neither
  tunable nor visible in the pool inventory"
status: resolved
resolved_in: 5af7e1e44
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

## Fix

`ZENOH_TX_BUF` is now generated, not literal:

* **knob** `ZPICO_PUBLISHER_TX_BUFFER_SIZE`, default **1024** — read by
  `packages/rmw/zenoh/nros-rmw-zenoh/build.rs` through the same `env_usize`
  helper as its subscriber-side twin `ZPICO_SUBSCRIBER_BUFFER_SIZE`, emitted
  into `buffer_config.rs` as `PUBLISHER_TX_BUFFER_SIZE`, and re-exported from
  `shim` beside `SUBSCRIBER_BUFFER_SIZE`. `shim::publisher::ZENOH_TX_BUF` keeps
  its name and is now an alias of that const, so no consumer spelling moves.
* **price** — `// nros-pool: PUBLISHER_TX_ARENAS = ZPICO_MAX_PUBLISHERS *
  ZPICO_PUBLISHER_TX_BUFFER_SIZE` beside `LendArena`. At defaults that is
  8 × 1024 = **8,192 bytes**, which is what the inventory now reports instead
  of nothing.

Env-only, like eight of the ten knobs that build script reads. It is not
forwarded from Kconfig: `_nros_resolve_knob` in
`zephyr/cmake/nros_cargo_build.cmake` carries only the two knobs with a
`KCONFIG_KNOBS` row, and adding a third means a `CONFIG_NROS_*` symbol plus a
cmake row — a separate change, and `check-kconfig-knob-forwarding` is
one-directional (cmake → reader), so an env-only knob does not trip it. A
Zephyr image that needs a bigger loan arena sets the env var at build time.

### Amendment — the knob is priced, the pool row is not (2026-08-27)

The first version of this fix also annotated `LendArena` with
`// nros-pool: PUBLISHER_TX_ARENAS = ZPICO_MAX_PUBLISHERS * ZPICO_PUBLISHER_TX_BUFFER_SIZE`,
which the inventory priced at 8,192 bytes. That row was removed before landing.

The arena exists only under the `lending` feature. Per
[issue 0814](0814-lending-never-exercised-on-hardware.md) that feature is
enabled by exactly one posix test crate and by no shipped image, and `nm` on a
built zenoh example confirms it: zero symbols matching `LendArena` or
`TX_ARENA`. Publishing 8,192 bytes of cost in the page people use to rightsize a
board, for storage their image does not contain, is the failure mode the
inventory exists to prevent — the same "a number that is right for one build and
wrong for the rest" that made [issue 0739](0739-static-pool-inventory-not-enumerable.md)
decline to annotate `MESSAGE_INFO_TABLE`.

The knob itself is enumerated, which is what this issue asked for: the ceiling is
now tunable and visible. Annotate the pool when `lending` reaches a shipped
image, and verify the figure against a real one with `just mem-report` rather
than trusting the arithmetic — see
[phase 394](../roadmap/phase-394-memory-campaign-ledger.md).
