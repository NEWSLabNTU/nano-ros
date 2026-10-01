---
id: 815
title: "The static-pool inventory finds 46 sizing knobs and can price 3, so the
  largest pools in a real image carry no byte figure"
status: resolved
resolved: 2026-10-01
type: tech-debt
area: tooling
related: [issue-0813, phase-392]
---

## Problem

`scripts/gen-pool-inventory.py` exists because of issue 0271, whose lesson was
*"the durable fix is not more knobs, it is making the existing ones
enumerable"*. Current output:

```
wrote book/src/reference/static-pool-inventory.md — 46 knob(s), 3 pool(s)
```

Bytes are opt-in: a pool declares its own arithmetic in a comment and the tool
evaluates it at the knobs' defaults.

```rust
// nros-pool: SMALL_PAYLOADS = ZPICO_MAX_SUBSCRIBERS * ZPICO_SUBSCRIBER_RING_DEPTH * ZPICO_SUBSCRIBER_BUFFER_SIZE
```

Three pools have done that. Forty-three knobs have not, so the reference page
lists them with no cost.

## What that hides, measured

Top RAM consumers in the mr_canhubk3/s32k344 safety-island image, by symbol
size. Bold rows are pools with **no** byte figure in the inventory:

| bytes | symbol | priced? |
| --- | --- | --- |
| 49,152 | `nros_rmw_zenoh::shim::subscriber::SMALL_PAYLOADS` | yes |
| **30,080** | **`__nros_comp_buf_0..3`** (C++ component placement-new storage) | **no** |
| **17,712** | **`nros_rmw_zenoh::shim::service::SERVICE_BUFFERS`** | **no** |
| **12,288** | **`nros_rmw_cffi::rust_adapter::static_subscriber_storage::SLOTS`** | **no** |
| 8,192 | `LARGE_PAYLOADS` | yes |
| **3,584** | **`nros_rmw_cffi::MESSAGE_INFO_TABLE`** | **no** |
| **2,640** | **`SUBSCRIBER_BUFFERS`** | **no** |

**66,304 bytes of unpriced pools** in one image — more than the 57,344 that
IS priced. A consumer reading the inventory to rightsize a board sees the
smaller half.

## Note on `__nros_comp_buf_N`

These are generated, not hand-written:

```rust
// packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs:390
"alignas(::{cls}) static unsigned char __nros_comp_buf_{i}[sizeof(::{cls})];"
```

So their size is `sizeof(component class)` — driven by the message types the
component embeds, which is driven by per-field storage mode. They cannot carry
a static `nros-pool:` line; the generator has to emit the figure. That is a
different mechanism from the annotation and belongs in the same phase.

## Resolution

Exact over guessed: no pool is priced by a hand-typed multiplier. Three
changes, all in `scripts/gen-pool-inventory.py` and the files it reads.

**1. The scan had lost knobs again.** `env_usize_rung(name, <declared rung>,
builtin)` — the DECLARED road of issues 1122/1199 — was the fifth reader
wrapper the scan did not know, and it had silently re-unpriced the pool this
issue's ancestor (0271) was filed about: `LARGE_PAYLOADS` read
`— (unknown knob ZPICO_MAX_LARGE_SUBSCRIBERS)`, and four knobs
(`ZPICO_MAX_LARGE_SUBSCRIBERS`, `ZPICO_SUBSCRIBER_LARGE_SIZE`,
`NROS_SERVICE_INBOX_BYTES`, `NROS_ACTION_INBOX_BYTES`) were absent. `nros`'s
three-argument `env_usize(name, rung, builtin)` hid the builtins of the four
`NROS_RUNTIME_*` knobs the same way. Both spellings are now patterns, each with
a self-test probe line. Knobs read on the declared road are marked DERIVED:
the page prices their pool at the builtin ("an image that declares its entities
derives its own"), and `mem-report --check` reports rather than fails such a
pool when an image measures differently.

**2. Every knob states its cost.** `scripts/pool-inventory-knobs.txt` gives
each knob no formula prices a kind (`pool`, `executor`, `component`, `heap`,
`stack`, `element`, `input`, `not-a-size`, `test-only`), the reason, and for a
`pool` the static(s) it sizes. The page's knob table has a `cost` column that
is never blank. The generator — which is the `pool-inventory` gate — FAILS on
an unclassified knob, a stale row, or a named symbol no tracked source defines;
each failure is a self-tested negative control.

**3. Pricing per image, from the image.** The pools whose element is a struct
(`SERVICE_BUFFERS`, `USER_SERVICE_INBOX`, `MESSAGE_INFO_TABLE`, `g_sessions`,
`TL_SLOTS`, …) are named in a new "measured per image" table, and
`mem-report` joins them by full symbol key and prints their exact bytes beside
their knobs. The C/C++ executor storage is priced from the build's own sizes
header (issue 1147). nros-smoltcp's four socket byte-buffers got `nros-pool:`
formulas (their knobs have per-platform defaults, so they price per image).

### Measured

| | before (this tree) | after |
| --- | ---: | ---: |
| knobs found | 79 | 83 |
| knobs with no cost statement (no formula names them) | 74 | **0** |
| formula pools / priced at defaults | 3 / 1 | 7 / 2 (`SLOTS` 8,192; `LARGE_PAYLOADS` 131,072) |
| statics joined to their knobs for per-image measurement | 0 | 23 |

On the FreeRTOS mps2-an385 C tiered image (`workspace-c-freertos-realtime`,
built from this branch) `mem-report` now names and measures, beside their knobs:
`ucHeap` 3,145,728, `g_sessions` 84,528, `USER_SERVICE_INBOX` 33,152,
`SMALL_PAYLOADS` 32,768, `MESSAGE_INFO_TABLE` 4,096, `TL_SLOTS` 2,680,
`SERVICE_BUFFERS` 2,400, `SUBSCRIBER_BUFFERS` 1,760, `NODE_TABLE` 544; and
`--check` reports `LARGE_PAYLOADS` 131,072 and `SLOTS` 8,192 agreeing with
their formulas. The issue's two named gaps: `SERVICE_BUFFERS` /
`MESSAGE_INFO_TABLE` / `SLOTS` are now joined and measured, and
`__nros_comp_buf_N` is attributed to `[component storage]` (its size is
`sizeof(class)`, which the image states; codegen does not need to emit a figure
for `mem-report` to report it exactly).

Sweep: `git grep -n -E 'env_usize(_rung|_min|_compat)?\(|knob\(' -- 'packages/*.rs'`.

### Not done

- No new static FIGURE for a struct pool: a byte figure at defaults would need
  `sizeof`, i.e. a build. The page says so per pool instead.
- The knobs whose bytes land on a heap (`NROS_MAX_PARAMETERS` boxes
  `ParameterStorage`; zenoh-pico's batch/frag buffers; the XRCE session state)
  have no symbol, so no tool here measures them — recorded as `heap` with the
  reason.
- The safety-island image (mr_canhubk3) in this issue's table was not rebuilt.
