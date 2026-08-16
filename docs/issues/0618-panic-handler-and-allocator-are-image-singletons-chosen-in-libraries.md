---
id: 618
title: "`#[panic_handler]` and `#[global_allocator]` are per-IMAGE singletons,
  but nano-ros decides them in libraries — so images get two or none"
status: open
type: design
area: build/api
related: [issue-0617, issue-0615, issue-0594, issue-0591, issue-0566, rfc-0034]
---

## Owner

The rule this issue disputes is normative, not accidental. `ARCHITECTURE.md` §2
("The `std` / `alloc` contract") says:

> Orthogonally: **`malloc` and `panic` are unified per platform.** Exactly one
> `#[global_allocator]` and one `#[panic_handler]` per image, selected by the
> `platform-<rtos>` feature — which selects the provider and nothing else.

implemented by **phase-361**, which quotes it verbatim as the rule every crate
feature table points at. So this is a disagreement with the architecture, and
changing it means amending §2 — the argument for that is RFC-0077.

Note two of the three clauses are not in dispute: *exactly one per image* and
*malloc and panic move together* are both right and both kept. Only *selected by
the `platform-<rtos>` feature* is challenged.

## The defect in one sentence

`#[panic_handler]` and `#[global_allocator]` are link-time singletons of the
FINAL ARTIFACT, and nano-ros selects them in library crates keyed on the
PLATFORM — so "exactly one per image" is not a property the build guarantees,
it is an invariant maintained by hand at every dep-site.

## Evidence that it is not holding

Both halves of issue 0617 are this, and they are the two failure modes the
design permits:

- **Two providers.** `#[global_allocator] in nros_platform conflicts with
  global allocator in: nros_platform` — the mixed Zephyr entry.
- **No provider.** `#[panic_handler] function required, but not found` —
  `nros-c` for `armv7a-nuttx-eabihf`.

Issue 0615 is the same shape one level up: a gate reasoned about dep-sites,
concluded `nros-cpp`'s `default = ["panic-spin"]` was unreachable, and asking
for it to be emptied would have removed a provider a staticlib needs.

## How the invariant is currently held

`packages/boards/nros-board-nuttx/src/lib.rs` states it plainly, and states the
mechanism:

> Exactly one `#[panic_handler]` may exist per image, and `nros-c` supplies one
> for `no_std` C/C++ images (its own gate: `global-allocator`, not `std`, not
> `panic-halt`). Both crates are linked into a C/C++ NuttX image, so the two
> would be a duplicate-lang-item link error. Those images therefore take this
> crate with `default-features = false` and let `nros-c` own the image runtime;
> a pure-Rust image links no `nros-c` and takes this handler.

So correctness depends on every consumer knowing which SHAPE of image it is
building (C/C++ umbrella vs pure-Rust) and disabling the right crate's default
features to match. That is a cross-crate, per-image-shape rule with no
mechanical check. Providers today:

| provider | gate |
| --- | --- |
| `nros-c` spin loop (`src/lib.rs`) | `panic-spin` && !`std` && !`panic-halt` |
| `panic-halt` crate | `panic-halt` feature |
| `nros-board-nuttx` | `target_os = "nuttx"` && `image-runtime` |
| `nros-board-threadx-qemu-riscv64` | **ungated** |
| `nros-board-mps2-an385-freertos` | board owns it (issue #45) |
| libstd | whenever `std` is on |

Six sources, five different gating idioms, and the composition rule lives in
prose.

## Why keying it on the platform cannot work

`nros-c`'s manifest says the intent directly:

> Like `global-allocator`, this is a PLATFORM statement — the `platform-*`
> features below select it, so malloc and panic stay unified per platform.

The platform does not know the image's policy. Two images for the SAME platform
legitimately want different handlers: a test fixture wants "print and exit(1)"
so the harness can grep it, a shipped controller wants "log to NVM, then
reboot", a bring-up image wants "spin so the debugger can attach". Issue 0594
already separated panic from the allocator because they are different facts;
this is the next step of the same correction — panic is not a platform fact
either.

The current default even apologises for it in `nros-c/src/lib.rs`:

> A halt+reboot would be ideal but needs port-specific config … looping is the
> safest `no_std`-compatible default.

That is a library choosing a policy it cannot know, because nothing lets the
image choose instead.

## What the shape should be

The rule the Rust ecosystem settled on, and the one this repo is missing:

**A library crate never defines `#[panic_handler]` or `#[global_allocator]`.
The final artifact does.**

For nano-ros that means:

1. **Libraries stop providing.** `nros-c`'s spin loop and the board crates'
   handlers move out of the rlib path.
2. **The image chooses, in the user's project.** A user writes
   `panic-halt = "…"` + `use panic_halt as _`, or their own
   `#[panic_handler]`, exactly as any embedded Rust project does — and nano-ros
   documents that as a required decision rather than one it silently makes.
3. **The entry layer supplies the default, not a library.** `nros::main!` /
   `nano_ros_entry()` generate code that IS part of the final artifact, so a
   convenience default belongs there, selectable and overridable, where today it
   is baked into a dependency.

**The staticlib wrinkle, which is why the naive rule is not enough.** `nros-c`
and `nros-cpp` build `crate-type = ["staticlib"]`, and rustc treats a staticlib
as a final artifact — it genuinely needs a panic handler when built standalone,
which is what 0615 discovered. So the rule needs one qualification: the provider
is required when the staticlib IS the deliverable (a C/C++ image links it and
nothing else), and must be absent when the staticlib is one input among several
Rust crates in a larger image. That distinction is knowable at the dep-site and
is exactly what nobody is currently checking.

## A third instance, found independently

Issue 0619's author reached this framing from another direction while fixing a
`cargo test` link failure, and extended it: the platform SYMBOL SET is a third
image singleton, and *"a dev-dependency is a library-level lever — it cannot
express 'supply these only when nobody else does'."* Three singletons, one
missing concept.

## A "none" instance, measured 2026-08-16 (phase-359 W10)

The examples above are the "two" half. Here is the "none" half, from a routine
manifest change rather than a link failure.

phase-359 W10 moved `examples/threadx-linux/rust/*` from `std` to `alloc` — the
right call on the merits, since ThreadX-Linux is a no_std board that happens to
run as a Linux process. `rust-rtos-link-check` then failed:

```
error: no global memory allocator found but one is required
```

and, once `nros-board-threadx-linux` was given
`image-runtime = ["nros-platform/global-allocator"]`:

```
error: `#[panic_handler]` function required, but not found
```

Both items had been arriving from libstd purely because the leaf's feature list
happened to say `std`. Nothing in the platform selection supplied them: this
image names `nros-platform/platform-threadx`, which per §2 is supposed to be
what selects the pair, and it selected neither. That is the clause this issue
disputes, failing in the direction the other instances do not show.

Two further points for the fix's design:

- The two errors appear ONE AT A TIME. Supplying the allocator is what reveals
  the panic handler is missing too, so any migration doing this per-crate pays
  two round trips per image and a reviewer sees only the first.
- `just check` is blind to it. A missing lang item is only an error in a FINAL
  artifact, and the fast lane never builds these leaves as one — this was green
  through three `just check` runs before `rust-rtos-link-check` caught it. The
  gate suggested below wants that property: it must look at images, not crates.

The six leaves are reverted to `std` with the reason recorded at each, pending
this issue and phase-366's move of board panic paths onto `nros_platform_panic`.

## Suggested gate

Half of it is now checked: `check-archive-lang-items` enforces "at most ONE Rust
archive per link line may define the global allocator". The panic half is not,
and neither is checked per image coordinate:
for each buildable image coordinate, count the crates in the resolved graph that
can emit `#[panic_handler]` under the selected features, and require exactly
one. That is the check that would have caught both halves of 0617 before either
took out a fixture family — and, per 0615's lesson, it must reason about FINAL
ARTIFACTS rather than dep-sites.

## Why a host build never sees any of this

`std` supplies both singletons, so every failure in this class is invisible on a
host lane and appears only on an embedded target — the same asymmetry recorded
in 0582 and 0617. Any gate for it has to run against embedded coordinates.
