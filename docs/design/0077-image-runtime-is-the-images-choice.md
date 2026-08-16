---
rfc: 0077
title: "The image runtime is the image's choice"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: [issue-0618, issue-0617, issue-0615]
amends: [ARCHITECTURE.md#2-the-std-alloc-contract]
supersedes: []
superseded-by: null
---

# RFC-0077 — The image runtime is the image's choice

## Summary

`#[panic_handler]` and `#[global_allocator]` are link-time singletons of the
FINAL ARTIFACT. nano-ros selects them in LIBRARY crates, keyed on the PLATFORM,
and every candidate provider defaults ON — so an image is correct only if its
author turns off the N−1 providers it did not want, by hand, differently for
each shape of image.

That is not a naming problem. It is why issue 0617 has both failure modes at
once: an image with **two** providers (`#[global_allocator] in nros_platform
conflicts with global allocator in: nros_platform`) and an image with **none**
(`#[panic_handler] function required, but not found`). Issue 0615 is the same
defect one level up — a gate concluded a `default = ["panic-spin"]` was
unreachable, and acting on it would have removed a provider a staticlib needs.

The concept nano-ros needs already exists in the tree, under the right name.
`nros-board-nuttx` calls it `image-runtime` and states the invariant exactly:

> Both live behind ONE feature on purpose. They are not two decisions — an image
> that gets its allocator from `nros-c` gets its panic handler from there too,
> and splitting them into two flags would let a build pick one of each and
> duplicate a lang item.

**This RFC does not introduce that idea. It inverts its direction:** stop
defaulting every candidate ON and negating the losers, and let the IMAGE name
its one owner, once.

## What this contradicts, and who owns it today

This is not a gap in the design. It is a disagreement with a rule that is
already normative, and adopting this RFC means amending `ARCHITECTURE.md` §2
("The `std` / `alloc` contract"), which says:

> Orthogonally: **`malloc` and `panic` are unified per platform.** Exactly one
> `#[global_allocator]` and one `#[panic_handler]` per image, selected by the
> `platform-<rtos>` feature — which selects the provider and nothing else.

Owner: that section, implemented by **phase-361** (which quotes it verbatim as
the rule every crate feature table points at).

Two of its three clauses are right and this RFC keeps them. *Exactly one per
image* is the invariant; *malloc and panic move together* is the coupling
`nros-board-nuttx` independently re-derived. The clause this RFC rejects is the
third — **selected by the `platform-<rtos>` feature** — on the grounds that the
platform is the one participant that cannot know the answer, and that the two
observed failure modes follow from putting the choice there.

Read narrowly, §2 is also already falsified by the tree it governs: it says
exactly one provider per image "selected by the `platform-<rtos>` feature", and
`nros-board-threadx-qemu-riscv64` defines an **ungated** `#[panic_handler]`
that no platform feature selects. That is a bug against §2, not evidence for
this RFC — but it shows the rule is not self-enforcing, which is why the gate
below matters whichever way the selection clause is settled.

**Credit where the mechanism already exists.** `check-archive-lang-items`
("at most ONE Rust archive per LINK LINE may define the global allocator")
already implements this RFC's gate for the allocator half, per link line. The
gate proposed here is that check extended to `#[panic_handler]` and reasoned per
image coordinate rather than per archive — a smaller step than it first appears,
and evidence the enforcement shape is workable.

## The evidence

### Six providers, five gating idioms

| provider | gate |
| --- | --- |
| `nros-c` spin loop (`src/lib.rs`) | `panic-spin` && !`std` && !`panic-halt` |
| `panic-halt` crate | `panic-halt` feature |
| `nros-board-nuttx` | `target_os = "nuttx"` && `image-runtime` |
| `nros-board-threadx-qemu-riscv64` | **ungated** |
| `nros-board-mps2-an385-freertos` | board owns it (issue #45) |
| libstd | whenever `std` is on |

### Three independent deciders, which must agree

1. `nros-c`'s `platform-*` features select `panic-spin`.
2. `cmake/NanoRosFeatureSet.cmake` appends `panic-halt` per platform tier.
3. Board crates carry their own, defaulted ON.

They are reconciled by a precedence rule written in prose (`panic-halt` beats
`panic-spin`, `std` supersedes both) and by consumers negating defaults.

### The composition rule lives in a doc comment

`nros-board-nuttx/src/lib.rs`:

> Exactly one `#[panic_handler]` may exist per image, and `nros-c` supplies one
> for `no_std` C/C++ images. Both crates are linked into a C/C++ NuttX image, so
> the two would be a duplicate-lang-item link error. Those images therefore take
> this crate with `default-features = false` and let `nros-c` own the image
> runtime; a pure-Rust image links no `nros-c` and takes this handler.

Correctness therefore depends on every consumer knowing which SHAPE of image it
is building. Nothing checks it.

## Why keying on the platform cannot work

The platform does not know the image's policy, and three images on one platform
legitimately want three different ones:

- a **test fixture** wants print-then-`exit(1)`, because the harness greps the
  message and the exit status;
- a **shipped controller** wants log-to-NVM-then-reboot;
- a **bring-up image** wants a spin loop, so a debugger can attach to a live
  core.

`nros-c` already concedes this in the source, while doing it anyway:

> A halt+reboot would be ideal but needs port-specific config … looping is the
> safest `no_std`-compatible default.

That is a library apologising for a policy only the image can choose. Issue 0594
already separated panic from the allocator because they are different facts;
this is the next step of the same correction — panic is not a *platform* fact
either.

## The existence proof

One platform already does this correctly, and it is the one where the upstream
ecosystem forced the question. `examples/qemu-esp32-baremetal/rust/talker`:

```toml
esp-backtrace = { version = "~0.18.0", features = ["esp32c3", "panic-handler", "println"] }
```

```rust
#![no_std]
use esp_backtrace as _;
nros::main!();
```

and `main_macro.rs` records the division of labour as a fact rather than a
workaround: *"The Entry crate provides the panic handler (`esp-backtrace`) and
app descriptor."*

So the proposed UX is not hypothetical and needs no new mechanism. It is what
one platform does today, what every embedded Rust project does, and what the
other platforms are prevented from doing.

## The proposed UX

### What a user writes

The image's own crate declares its runtime, in one visible line:

```rust
#![no_std]
use panic_halt as _;        // or panic_semihosting, or esp_backtrace, or your own
nros::main!();
```

```toml
[dependencies]
panic-halt = "1"
```

A user who wants their own writes it, and nothing competes:

```rust
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    nvm::record(info);
    board::reboot()
}
```

### What `nros new` scaffolds

The default must be **visible and editable**, not invisible and fought. Entry
scaffolding emits the two lines above, with the provider taken from the board
descriptor's recommendation (below). The user then sees, in their own crate, the
decision they are free to change — which is the difference between a default and
a constraint.

### What the board descriptor says

The board keeps its knowledge without keeping ownership. `nros-board.toml`
declares a RECOMMENDATION, consumed by scaffolding and by nothing at link time:

```toml
[board.image_runtime]
recommended_panic = "panic-halt"     # what `nros new` writes into the entry
```

Board knowledge stays in board data (RFC-0064's direction) while the binding
choice moves to the image.

## The design

1. **Libraries provide nothing.** `nros-c`'s spin loop and the board crates'
   handlers leave the rlib path. A crate that is only ever linked INTO an image
   never claims a lang item.

2. **The image names one owner, positively.** `image-runtime` stops being a
   default-ON feature on several crates that consumers negate, and becomes a
   single statement made once per image. Panic and allocator stay behind that
   one flag, for the reason `nros-board-nuttx` already gives: they are one
   decision, and two flags would let a build take one of each.

3. **The entry layer materialises it.** `nros::main!` and `nano_ros_entry()`
   generate code that IS part of the final artifact, which is the only place in
   nano-ros that can legitimately supply a default. A dependency cannot.

4. **The staticlib qualification — why the naive rule is not enough.**
   `nros-c` and `nros-cpp` build `crate-type = ["staticlib"]`, and rustc treats
   a staticlib as a final artifact. When the staticlib IS the deliverable (a
   C/C++ image links it and nothing else) it genuinely needs a provider; when it
   is one input among several Rust crates it must not have one. That is exactly
   what issue 0615 discovered, and it is knowable at the dep-site: the C/C++
   build path knows it is producing the image. So the flag is set by the entry
   layer for that path, not by `platform-*` for all paths.

## The gate

Half of this already exists: `check-archive-lang-items` enforces "at most ONE
Rust archive per link line may define the global allocator". What is missing is
the panic half, and a coordinate-level view. Per buildable image coordinate: count the crates in the resolved graph that can
emit `#[panic_handler]` (or `#[global_allocator]`) under the selected features,
and require exactly one.

Two constraints on that gate, both learned the hard way:

- It must reason about **final artifacts**, not dep-sites — clause (d) of
  `check-feature-contract` reasoned about dep-sites and asked for a provider to
  be deleted (issue 0615).
- It must run against **embedded coordinates**. `std` supplies both singletons,
  so every defect in this class is invisible on a host lane — the same asymmetry
  recorded in 0582 and 0617.

## Migration

The six providers cannot move at once without a window where some image has
none. Ordering that keeps the tree green:

1. Add the gate first, reporting only. It names today's true owner per
   coordinate, which is the inventory this RFC could not otherwise trust.
2. Add the positive `image-runtime` selection and have the entry layer set it,
   with the existing defaults still ON — no image changes owner yet.
3. Flip defaults OFF one provider at a time, gate enforcing, starting with the
   ungated `nros-board-threadx-qemu-riscv64` because it is the one that cannot
   currently be turned off at all.
4. Scaffold the user-visible lines; update the book's embedded pages, where the
   panic decision is currently never mentioned because it never had to be.

## Alternatives considered

**Keep platform-keying, add a conflict gate.** Cheaper, and it would catch 0617.
But it leaves the user unable to choose a handler at all — the defect a fixture
that wants print-and-exit hits today — so it fixes the symptom and keeps the
cause.

**One nano-ros-owned handler for every image.** Simplest to reason about, and
wrong for the same reason: it is a policy, and policies belong to the image.
It also cannot serve ESP32, whose ecosystem supplies its own.

**Split panic and allocator into separate image flags.** Rejected on the
evidence already in the tree: `nros-board-nuttx` explains that they are one
decision and two flags would let a build take one of each and duplicate a lang
item.
