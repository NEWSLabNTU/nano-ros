---
id: 739
title: "Static pools each add a sizing knob silently, so a correctly-rightsized image inherits defaults nobody knew to change"
status: resolved
type: enhancement
opened: 2026-08-21
resolved: 2026-08-21
severity: medium
area: embedded, docs, build
related: [issue-0271, issue-0269, rfc-0038]
---

# 0739 — the knobs existed; nobody could enumerate them

Split out of issue 0271, which found this and named it as the durable fix.

## The finding, restated from 0271's audit

A 256 KB-class image (Orin SPE, `Executor::open` + `spin`) overflowed its BTCM
by 168,760 bytes. 91 % of that was recovered, and **four of the five wins were
the same shape: the knob already existed and the consumer did not know.**

That build tuned NINE environment knobs, with a comment explaining each, and
still inherited ~145 KB of defaults across four separate features — because each
feature that added a static pool added its knob silently. The largest single
item:

```
ZPICO_MAX_LARGE_SUBSCRIBERS(2) x ZPICO_SUBSCRIBER_RING_DEPTH(4)
    x ZPICO_SUBSCRIBER_LARGE_SIZE(16384)  =  131,072 bytes
```

Only ONE item needed nano-ros code (`NROS_RMW_MESSAGE_INFO_SLOTS`, hardcoded at
64 while every neighbour was env-tunable). The rest was a discoverability
failure, not a defaults failure. 0271's conclusion:

> the durable fix is not more knobs — it is making the existing ones
> enumerable … Worth more than any individual knob, and it is the thing this
> issue most argues for.

Measured 2026-08-21, before this landed: **zero** of the five knobs that audit
needed appeared in `book/src/reference/environment-variables.md`.

## Fix

`scripts/gen-pool-inventory.py` generates
`book/src/reference/static-pool-inventory.md`; `just check pool-inventory`
(fast lane) fails when it drifts. Generated rather than written, because a
hand-kept list goes stale the first time a feature lands — which is the defect
itself, one level up.

**34 knobs** are enumerated mechanically: every `env_usize` /
`env_usize_compat` / `knob_usize` / `env::var(..).unwrap_or_else(..)` call site
carries its default as a literal argument, so name, default and owning crate
are all recoverable without building anything.

## What is deliberately NOT claimed

Bytes are not mechanical. A pool is a `static mut [[[u8; A]; B]; C]` over
generated consts from several crates; resolving that needs a compiler. So byte
figures are OPT-IN — a pool declares its own arithmetic:

```rust
// nros-pool: LARGE_PAYLOADS = ZPICO_MAX_LARGE_SUBSCRIBERS \
//   * ZPICO_SUBSCRIBER_RING_DEPTH * ZPICO_SUBSCRIBER_LARGE_SIZE
```

and the generator evaluates it at the knobs' defaults. An unannotated knob still
gets a row with its default, and the page says it carries no byte figure —
rather than implying it is free.

`MESSAGE_INFO_TABLE` is the worked example of the refusal: `MessageInfoSlot`'s
width depends on cfg (`alloc` + `safety-e2e` add three fields), so any constant
would be right for one build and wrong for the rest. 0271 measured 3,584 bytes
at 64 slots in ITS configuration; publishing that as the cost would be exactly
the fabrication this page exists to avoid. The source carries that reasoning.

## Verification

The generator independently reproduces 0271's measured figures, which is the
strongest check available — the numbers came from a link map, this from source:

| pool | inventory, at defaults | 0271 measured |
| --- | ---: | ---: |
| `LARGE_PAYLOADS` | 131,072 | 131,072 ("matching the pool exactly") |
| `SLOTS` | 8,192 | 8,192 |

Also verified: the gate fails on a drifted page (rc=1) and passes when
regenerated; the generator self-tests both directions every run (a product at
defaults, and an unknown knob refusing to evaluate rather than silently
vanishing).

A conflicting-defaults detector is included — one knob read in two places with
different defaults means setting it moves half the tree, the issue-0135
split-brain shape one layer up. **No conflicts exist today**; the section is
omitted rather than printed empty.

## Not covered

Knobs read by C code (`#define` fallbacks in zenoh-pico's own build) are not
scanned; only the Rust call sites that set them are. And the inventory says
what a knob COSTS, never what an image can afford — sizing an image is still a
link-map job (0271's own false lead was reading an archive instead of a link).
