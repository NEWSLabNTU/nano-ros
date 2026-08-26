---
id: 815
title: "The static-pool inventory finds 46 sizing knobs and can price 3, so the
  largest pools in a real image carry no byte figure"
status: open
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
