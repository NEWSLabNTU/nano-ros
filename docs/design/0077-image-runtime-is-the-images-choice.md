---
rfc: 0077
title: "The image runtime is the image's choice"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: [issue-0618, issue-0617, issue-0615]
amends: [ARCHITECTURE.md#2-the-std-alloc-contract]  # §2 predates issue-0616; see "Where the design already is"
supersedes: []
superseded-by: null
---

# RFC-0077 — The image runtime is the image's choice

## Summary

`#[panic_handler]` and `#[global_allocator]` are link-time singletons of the
FINAL ARTIFACT. The tree has already accepted that for the ALLOCATOR — issue
0616 established it, and `check-archive-lang-items` now enforces one per link
line. **This RFC finishes the job for the PANIC HANDLER, which is the harder
half, and says why it is harder.**

Three concerns are collapsed into one rule today: the IMPLEMENTATION (how malloc
and panic work on this port), the INSTALLATION (which artifact carries the lang
item), and the POLICY (what a panic should actually do). The first is the
platform's and `ARCHITECTURE.md` §2 has it right. The second is the link root's
and 0616 settled it. The third belongs to the image and currently has no owner
at all — which is why a fixture that wants print-and-exit and a controller that
wants log-then-reboot get the same spin loop, and why issue 0617 has both an
image with two providers and an image with none.

The asymmetry is the crux: **for the allocator, implementation IS policy** —
"use this platform's heap" is the only sensible answer, so keying it on the
platform costs nothing. **For panic they are different facts**, every behaviour
is implementable everywhere, and keying it on the platform costs the choice
itself.

Proposed: keep implementation platform-keyed; keep installation coupled and
owned by one link root, exactly as `nros-board-nuttx` argues; and let the IMAGE
name its panic policy in its own crate, the way `examples/qemu-esp32-baremetal`
already does with `use esp_backtrace as _;`.

## Where the design already is — this completes it, it does not oppose it

An earlier draft of this RFC framed the problem as a disagreement with
`ARCHITECTURE.md` §2. That framing was wrong, and correcting it is most of the
argument.

§2 says:

> Orthogonally: **`malloc` and `panic` are unified per platform.** Exactly one
> `#[global_allocator]` and one `#[panic_handler]` per image, selected by the
> `platform-<rtos>` feature — which selects the provider and nothing else.

That text landed with phase-360 W1/W4 (`d56ed1fe3`) and **predates issue 0616**,
which then established the opposite of its unstated premise. 0616's own words:

> `#[global_allocator]` is a lang item: **unique per LINKED ARTIFACT**. nano-ros
> declares it in `nros-platform`, a mid-graph library, gated on a feature — and
> issue 0594's guarantee, "cargo unifies one crate's one feature into one unit",
> is a property of ONE graph. A staticlib is not a graph; it is a sealed copy of
> one. Four sealed copies can each contain the item and each be individually
> correct.

and its fix options are this RFC's design, written first and independently:

> **One link root per image, enforced** … those lang items belong to whoever owns
> the image, and a backend does not.
>
> **Move the item to the root crate** … the `#[global_allocator]` STATIC is
> installed by the link root through a macro. "One per image" then means "one
> root", which the build system already controls, rather than "one unit", which
> it does not.
>
> **A link-side gate** … `nm` the produced archives and assert at most one
> defines `___rust_alloc` per image.

The third has landed as `check-archive-lang-items` ("at most ONE Rust archive per
LINK LINE may define the global allocator"). So the tree has already moved to a
per-image model **for the allocator**. What has not moved is the panic handler,
and §2's text, which still describes the pre-0616 world.

## The distinction §2 conflates

Three separable concerns are collapsed into one sentence, and separating them
dissolves the apparent conflict:

| concern | question | belongs to | today |
| --- | --- | --- | --- |
| **implementation** | *how* does malloc/panic work here? | the PLATFORM | §2, correct |
| **installation** | which artifact carries the lang item? | the LINK ROOT | 0616, gate landed for alloc |
| **policy** | what should a panic DO? | the IMAGE | nobody |

§2's "selected by the `platform-<rtos>` feature" is right about
**implementation** — a platform genuinely does determine that `k_malloc` rather
than `pvPortMalloc` backs the heap. It is silent on **installation**, which is
what 0616 had to discover the hard way. And it has no place at all for
**policy**, which is the gap this RFC exists to fill.

## Why the allocator and the panic handler are not symmetric

This is the part neither §2 nor 0616 nor `nros-board-nuttx` separates, and it is
why panic is the harder half.

**For the allocator, implementation IS policy.** "Use this platform's heap" is
the only sensible answer; there is no second reasonable choice for an image to
make. So keying the allocator on the platform loses nothing.

**For the panic handler they are different facts.** Spin, halt, print-and-exit,
log-to-NVM-then-reboot are all implementable on every platform, and which one is
right depends on what the image IS — a fixture whose harness greps the message,
a shipped controller, a bring-up image with a debugger attached. Keying panic on
the platform therefore does lose something: the choice itself.

Treating them as one decision is correct for **installation** and wrong for
**policy**. That distinction is what the current design is missing, and stating
it is this RFC's actual contribution — the allocator half is already settled.

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

Stated per layer, because the layers have different owners:

1. **Implementation stays platform-keyed.** `platform-<rtos>` continues to name
   the port that supplies malloc and panic mechanics. §2 is right here, and
   `check-platform-provider-features.py` (issue 0617) already enforces that
   every RTOS row names one — including that `platform-posix` must NOT, because
   libstd supplies both.

2. **Installation is the link root's, and stays coupled.** One artifact per
   image carries both lang items. This is `nros-board-nuttx`'s argument, kept
   intact: one owner, one switch, so no build can take one of each. What changes
   is direction — `image-runtime` stops being a default-ON feature on several
   crates that consumers must negate, and becomes a single positive statement
   made once per image. 0616's option (2) is the stronger form: `nros-platform`
   keeps providing the `GlobalAlloc` TYPE and the root installs the STATIC via
   `install_global_allocator!()`, so "one per image" means "one root" — which
   the build system controls — rather than "one unit", which it does not.

3. **Policy is the image's, and only for panic.** The image names what a panic
   does, in its own crate. The allocator needs no equivalent because its
   implementation is its policy.

4. **The entry layer materialises the default.** `nros::main!` and
   `nano_ros_entry()` generate code that IS part of the final artifact — the only
   place in nano-ros that can legitimately supply a default. A dependency cannot.

5. **The staticlib qualification.** `nros-c`/`nros-cpp` build
   `crate-type = ["staticlib"]`, and rustc treats a staticlib as a final
   artifact. When the staticlib IS the deliverable (a C/C++ image links it and
   nothing else) it needs a provider; when it is one input among several Rust
   crates it must not have one. That is what issue 0615 discovered, and it is
   knowable at the dep-site — the C/C++ build path knows it is producing the
   image. 0616 makes the same point from the other side: "a staticlib is not a
   graph; it is a sealed copy of one."

## The gate

The allocator half exists: `check-archive-lang-items` asserts at most one
archive per LINK LINE defines `__rust_alloc`. Two things are missing.

**The panic half.** The same script, the same link lines, `__rust_begin_short_backtrace`
being the wrong symbol to key on — `rust_begin_unwind` is the panic lang item's
external name and is what `nm` can see. Extending the existing check is a smaller
change than writing a new one.

**A coordinate-level view.** Per link line catches duplicates; it cannot catch
ABSENCE, because an image with no provider has no archive to count. Issue 0617's
`#[panic_handler] function required` was exactly that, and it is caught today
only by the build failing. Per buildable image coordinate, the count must be
exactly one — not at most one.

Two constraints, both learned here:

- Reason about **final artifacts**, not dep-sites — `check-feature-contract`
  clause (d) reasoned about dep-sites and asked for a provider a staticlib needs
  to be deleted (0615), and clause (e) counts source definitions when "the count
  it should be making is per produced archive" (0616's words).
- Run against **embedded** coordinates. `std` supplies both singletons, so the
  whole class is invisible on a host lane — 0617 records that NuttX's missing
  provider "was invisible for as long as NuttX images linked `std`".

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

**Split panic and allocator into separate image flags.** An earlier draft
rejected this, citing `nros-board-nuttx`: "two flags would let a build take one
of each and duplicate a lang item." That rejection was wrong, and the error is
instructive — it applies an INSTALLATION argument to a POLICY question. The
board's reasoning is sound about installation: one owner should install both,
or two crates can each install one. It says nothing about who chooses what a
panic does. So the answer is to couple INSTALLATION (one owner, one switch,
exactly as the board argues) and decouple POLICY (the image names the panic
behaviour; the allocator has only one sensible answer and stays with the
platform).
