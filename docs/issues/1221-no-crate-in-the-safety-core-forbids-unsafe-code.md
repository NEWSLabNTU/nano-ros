---
id: 1221
title: "Not one crate in `packages/{core,api,platform,boards,rmw,drivers,tooling}`
  denies or forbids `unsafe_code`, so the three core crates that are at zero
  unsafe are held there by nothing"
status: open
type: tech-debt
area: [core, ci]
related: [1208, 1212, 0196]
---

## What

The stated reason the core layer is Rust is safety. Measured, no crate in any
shipped layer carries a lint that enforces it:

```
$ grep -rn 'forbid(unsafe_code)\|deny(unsafe_code)' packages/ --include='*.rs'
packages/cli/nros-lang/src/lib.rs:37:#![forbid(unsafe_code)]
packages/cli/nros-msg-to-idl/src/lib.rs:8:#![forbid(unsafe_code)]
packages/cli/nros-entry-lower/src/lib.rs:26:#![forbid(unsafe_code)]
```

Three hits, all host CLI crates under `packages/cli/`. Zero in `core`, `api`,
`platform`, `boards`, `rmw`, `drivers`, `tooling`.

The unsafe-related attributes that *do* exist in those layers are the
`unsafe_op_in_unsafe_fn` hygiene lint, which governs *how* unsafe is written,
not whether it appears — and the majority of its occurrences are `allow`, not
`deny`:

| direction | sites |
| --- | --- |
| `#![allow(unsafe_op_in_unsafe_fn)]` | `packages/api/nros-c/src/lib.rs:25`, `packages/rmw/cffi/src/generated.rs:6`, `packages/boards/nros-board-cffi/src/generated.rs:6`, `packages/platform/nros-platform-cffi/src/generated.rs:6` |
| `#![deny(...)]` / `#![forbid(...)]` | `packages/core/nros-log/src/lib.rs:45`, `packages/boards/nros-board-linux/src/lib.rs:71`, `packages/drivers/net/{lan9118,openeth}-smoltcp/src/lib.rs` |

`nros-c`'s `allow` is deliberate and documented in CLAUDE.md; the generated-file
ones are by construction. That is not the problem. The problem is that the layer
whose whole justification is safety has no floor.

## What is free to ratchet today

Three core crates are at **literally zero** `unsafe` — not zero blocks, zero
occurrences of the token in `src/`:

| crate | src LOC | `unsafe` occurrences |
| --- | ---: | ---: |
| `packages/core/nros-serdes` | ~5,000 | **0** |
| `packages/core/nros-serdes-packed` | 435 | **0** |
| `packages/core/nros-diagnostics` | 182 | **0** |

Verified by `grep -rn unsafe <crate>/src --include='*.rs' | wc -l`. Each could
take `#![forbid(unsafe_code)]` in a one-line commit with no other change, and
`nros-serdes` is the CDR serializer — the crate where a pointer bug is a wire
bug, and the one most heavily covered by the Kani/Verus harnesses that assume it.

`packages/core/nros-core` is near-zero (2 blocks, both the platform-clock
`extern "C"` seam of issue 1208, plus 2 `unsafe impl Send/Sync for OnceFlag`) and
`packages/core/nros-params` has exactly 1 (`server.rs:170`,
`pub unsafe fn init_in_place`). Those two cannot forbid, but they could carry a
per-crate ceiling if anyone wanted one.

## Why the absence matters more than the count

The core is bimodal, and the average hides it. `nros-node` is 45k LOC — 59 % of
the core — and holds 370 of the core's 417 `unsafe` blocks and 108 of its 118
`unsafe fn`. So "the core has 5 unsafe sites per KLOC" is true and useless: the
other nine crates average ~2/KLOC and three of them are at zero, while one crate
carries essentially all of it.

Without a lint, the direction of travel is unrecorded. A `String::from_utf8_unchecked`
added to `nros-serdes` next month is a normal-looking diff that no gate objects
to, and the crate silently leaves the class it is currently in. `forbid` is the
only mechanism that makes that a build failure rather than a review question, and
it costs nothing on a crate already at zero.

Note this is explicitly **not** a proposal to reduce `nros-node`'s unsafe — that
is a design question (its unsafe is ~69 % a hand-rolled type-erased bump arena in
`executor/{arena,spin}.rs`, which exists to avoid `alloc`, which is the whole
point of the crate). It is a proposal to stop the crates that are already clean
from drifting.

## Fix sketch (not applied)

1. Add `#![forbid(unsafe_code)]` to `nros-serdes`, `nros-serdes-packed`,
   `nros-diagnostics`. Three lines, no behaviour change, verifiable by
   `cargo check`.
2. Decide and record a position for the rest of the core — even "these crates
   carry unsafe and here is the ceiling" is better than silence, because it makes
   the bimodality a stated design fact instead of something a reader has to
   measure.
3. If a gate is wanted rather than per-crate lints, the shape that fits this repo
   is a census-and-ratchet like `scripts/check-std-census.py`: per-crate unsafe
   counts, frozen, failing on an increase. That also covers the crates that
   cannot forbid, which per-crate lints cannot.
