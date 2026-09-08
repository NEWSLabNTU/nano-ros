---
id: 1221
title: "Not one crate in `packages/{core,api,platform,boards,rmw,drivers,tooling}`
  denies or forbids `unsafe_code`, so the three core crates that are at zero
  unsafe are held there by nothing"
status: resolved
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

## Resolution

### Re-measured first, 2026-09-08

The counts above are from 2026-09-07 and the tree moves, so a crate that had
since gained one `unsafe` would have taken a build break rather than a guard.
Re-run over every crate in the seven shipped layers
(`grep -rn unsafe <crate>/src --include='*.rs' | wc -l`): the three named are
still at literal zero, and **thirteen** crates are, not three — the issue
measured `packages/core` and stopped.

| crate | src LOC | `unsafe` | gated |
| --- | ---: | ---: | :-: |
| `core/nros-serdes` | 5 013 | 0 | yes |
| `core/nros-serdes-packed` | 434 | 0 | yes |
| `core/nros-diagnostics` | 181 | 0 | yes |
| `boards/nros-board-common` | 3 998 | 0 | yes |
| `boards/nros-board-threadx-port-riscv64` | 165 | 0 | yes |
| `tooling/nros-platform-config` | 4 268 | 0 | yes |
| `tooling/nros-cargo-profile` | 737 | 0 | yes |
| `tooling/nros-build-paths` | 418 | 0 | yes |
| `tooling/nros-cbindgen-headers` | 159 | 0 | yes |
| `tooling/nros-cc-flags` | 150 | 0 | yes |
| `boards/nros-board-s32z270-freertos` | 25 | 0 | **no** |
| `boards/nros-board-mps3-an536-freertos` | 29 | 0 | **no** |
| `rmw/cyclonedds/cyclonedds-sys` | 13 | 0 | **no** |

Nothing gained an `unsafe` since the filing, and nothing needed an `allow` to
qualify — the point of the exercise would be inverted by one.

### Zero is necessary, not sufficient

Ten of the thirteen took the lint. The three that did not are the ones whose
count is zero because the crate is **unfinished or is the FFI seam itself**,
where `forbid` would record an intention nobody has formed:

* `nros-board-s32z270-freertos` and `nros-board-mps3-an536-freertos` say so in
  their own module docs — *"This Rust side is deliberately minimal … A Rust
  entry lane (console writer, panic behaviour) lands with phase-372 W5."* A
  bare-metal console writer is MMIO. Forbidding now schedules its own removal.
* `cyclonedds-sys` is thirteen lines of doc comment and `#![no_std]`; a `-sys`
  crate exists to expose a C library, so a lint against `unsafe` there is a
  category error rather than a guard.

The ten that took it are crates whose ROLE implies the property, not just
crates whose count happens to be zero today: three pure data-transformation
core crates (`nros-serdes` is the CDR serializer — the crate where a pointer
bug is a wire bug, and the one the Kani/Verus harnesses assume), five host-side
build-script helper libraries that compute paths, flags and profiles, and two
board crates that carry no hardware access at all (`nros-board-common` is a
trait plus a manifest parser; `nros-board-threadx-port-riscv64` states that it
"ships no runtime Rust" and exposes the paths of six vendored `.S` files).

### `forbid` IS the gate, for these crates

A script was considered and rejected *for this population*, on four counts:

1. **It cannot be locally overridden.** `forbid` outranks a later `allow`, so
   an `#[allow(unsafe_code)]` inside the crate is itself an error. A census
   script's baseline file is a text file anyone can edit in the same commit.
2. **It fails in the author's own `cargo check`**, not in a lane. The nearest
   equivalent here is `check-fast`, which is where a class like this is
   discovered by whoever rebases, not by whoever wrote it.
3. **It has no reach to keep in step with the rule it enforces** — the 0196
   shape that this repo keeps re-finding in its own gates. There is no list of
   crates to forget to extend and no baseline to go stale; the lint is in the
   crate or it is not.
4. **It cannot be satisfied vacuously.** A census over a `grep` count is
   satisfied by a spelling change; the compiler's answer is about the code.

What a script would buy that `forbid` cannot is the *other* population — the
crates that carry unsafe and therefore cannot forbid. That is step 2/3 of the
sketch above and is deliberately NOT done here; see below.

### Proof the lint fires

`forbid` is only a guard if it is in the crate ROOT of the crate you think it
is. Measured, not read: a `let _v = unsafe { *_p };` probe appended to each
crate root, `cargo check -p <crate>`, then the file restored.

```
enforced      nros-serdes — error: usage of an `unsafe` block
enforced      nros-serdes-packed — error: usage of an `unsafe` block
enforced      nros-diagnostics — error: usage of an `unsafe` block
enforced      nros-board-common — error: usage of an `unsafe` block
enforced      nros-board-threadx-port-riscv64 — error: usage of an `unsafe` block
enforced      nros-cc-flags — error: usage of an `unsafe` block
enforced      nros-cbindgen-headers — error: usage of an `unsafe` block
enforced      nros-build-paths — error: usage of an `unsafe` block
enforced      nros-platform-config — error: usage of an `unsafe` block
enforced      nros-cargo-profile — error: usage of an `unsafe` block
```

Ten of ten. A crate whose probe compiled would have meant the attribute sat in
a file that is not the crate root.

### Builds, including bare metal

* `just check no-std` — green. It already covers `nros-serdes` and
  `nros-diagnostics` on **both** `thumbv7m-none-eabi` and
  `riscv32imc-unknown-none-elf`.
* `nros-serdes-packed` is not in that lane's crate list, so it was checked
  explicitly on both targets, together with `nros-board-common
  --no-default-features` (its `no_std` arm) on `thumbv7m-none-eabi`. Green.
* Host: `cargo check --all-targets` over all ten, plus `--all-features` on the
  two with non-trivial feature sets (`nros-serdes`'s `std`/`alloc`,
  `nros-board-common`'s `build-helpers`). Green.

### Not done here

**The other population — a census-and-ratchet over the crates that cannot
forbid.** `nros-node` (600 occurrences over 46 544 LOC), `nros-c` (1 057),
`nros-cpp` (984) and `rmw/cffi` (540) carry essentially all of the tree's
unsafe, and freezing those counts is a decision about the hand-rolled
type-erased bump arena in `executor/{arena,spin}.rs` — which exists to avoid
`alloc`, which is the point of the crate. This issue says so itself and is
explicit that it is not proposing to reduce it. A ratchet is worth having and
is a different piece of work with a different argument; it is filed separately
rather than left in an archived file where nothing looks.
