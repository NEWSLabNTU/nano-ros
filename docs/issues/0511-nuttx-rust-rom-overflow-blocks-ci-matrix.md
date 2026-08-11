---
id: 511
title: "`rust-rtos-link-check` overflows NuttX ROM by 538800 bytes, blocking `ci-matrix`"
status: open
type: bug
area: nuttx
related: [issue-0477, phase-345, phase-346, phase-336]
---

## Symptom

`just ci-matrix` fails at `rust-rtos-link-check`:

```
ld: .../talker-ad2ca5af12988e3d section `.text' will not fit in region `ROM'
ld: region `ROM' overflowed by 538800 bytes
error: could not compile `nuttx_rs_talker` (bin "talker")
error: recipe `rust-rtos-link-check` failed with exit code 101
error: recipe `ci-matrix` failed with exit code 101
```

The image is `examples/qemu-arm-nuttx/rust/talker`, profile
`nros-relwithdebinfo`, target `armv7a-nuttx-eabihf`, linked with
`-Tdramboot.ld` against the `nros-nuttx-export-arm` snapshot.

## What has been ruled out

**It is not phase-346 W1** (the proc-macro framework change). Reverting
`nros-macros/src/main_macro.rs` and `nros-orchestration-ir/src/lib.rs` to the
commit before it and rebuilding produced the **bit-identical** figure —
`overflowed by 538800 bytes`. NuttX resolves to `OwnedSpin` on both sides of
that change, so the emitted entry is the same; the measurement confirms it
rather than relying on the reasoning.

**It is not stale build state**, which is the one thing this symptom has meant
before. Issue 0477 was the same message (448776 bytes then) caused by a board
artifact staged against the wrong export tree, and its remedy does not apply
here:

| attempt | result |
| --- | --- |
| `cargo clean -p nros-board-nuttx-qemu -p nros-board-nuttx` in the leaf (0477's remedy) | still overflows |
| `rm -rf` the whole leaf `target/`, full rebuild | still overflows, same figure |
| ARM export present (`third-party/nuttx/nuttx/nros-nuttx-export-arm`) | yes, checked |

A figure that is byte-identical across a from-scratch rebuild and across a
source change is deterministic, not a stale-artifact race.

## Why it matters

`rust-rtos-link-check` is on the `ci-matrix` line, so tier 2 cannot go green for
anyone on this tree. It is also the gate that exists to catch exactly this class
before a platform sweep, so it is doing its job — what is missing is the cause.

## Where to look

* the size delta against the last known-good NuttX Rust image — 0477 records
  687112 bytes for the C talker as its good reference, so a comparable Rust
  number would say whether this is growth or a link-script/region change;
* `nros-relwithdebinfo` is an `[optimized + debuginfo]` profile: if the region
  budget assumed `nros-minsizerel`, the lane and the linker script disagree
  about which profile the ROM must fit (phase-336 made the profile a per-language
  knob, so the lane's choice is worth re-reading);
* whether the `dramboot.ld` region sizes changed with the export snapshot.

## Bisect (2026-08-11) — RETRACTED, it measured the wrong thing

> **The result previously recorded here — "the boundary is phase-338 W2's
> `-entry` collapse; #440 restored linkability but not the link configuration" —
> is WITHDRAWN. It was an artifact of the probe, not a property of the code.**

The probe built `examples/qemu-arm-nuttx/rust/talker` at every revision. That
path is not the same THING on both sides of the collapse:

| revision | what that directory is | what `cargo build` there produces |
| --- | --- | --- |
| before `ab486a8db` | the NODE package — `[lib] crate-type = ["rlib", "staticlib"]`, **no `[[bin]]`** | rlib + staticlib. **Nothing is linked into an executable** |
| from `ab486a8db` | the collapsed package — `[lib]` **and** `[[bin]]` | the image, which is linked, and overflows |

So every `FIT` before the collapse is **vacuous**: an image that is never linked
cannot overflow a ROM region. The "boundary" the bisect found is exactly the
commit where that directory GAINED a `[[bin]]` — the probe was measuring when
the path started producing a binary, which is the confounder CLAUDE.md warns
about and which this very issue cited when describing 0477.

**What survives the retraction:**

* the OVERFLOW verdicts, from `ab486a8db` onward. There the path does build an
  image, and it overflows at a constant 534704 bytes (ambient profile) — the
  constancy is still evidence of a switch rather than accumulation;
* **no known-good revision has been established for this image at all.** That is
  the important correction: this may not be a regression. The NuttX *Rust* image
  may never have fit. 0477's good reference (687112 bytes) is the **C** talker,
  a different image.

**What a valid probe must do**, for whoever picks this up:

1. build the ENTRY package at each revision — `talker-entry/` before the
   collapse, `talker/` after — not one fixed path;
2. control the NuttX export layout, which moved under this window: phase-339
   replaced the `staging/` tree with per-arch snapshots
   (`nros-nuttx-export-<arch>/`). Building a pre-339 revision against a post-339
   export fails with `cannot open linker script file dramboot.ld` — measured, and
   the reason the first attempt at a pre-collapse ENTRY build produced no image;
3. treat "does not link" as SKIP, never as good — the trap above in another form.

Until (1) and (2) are done there is no evidence that any revision of this image
ever fit, and therefore none that a commit broke it.

## Also found — FIXED 2026-08-11: the lane built with the wrong profile (~119 KB)

`rust-rtos-link-check` uses `nros_cargo_profile_arg_string` →
`--profile nros-relwithdebinfo`, whose own comment in `Cargo.toml` reads
`opt-level = 3   # Performance. Size lives in nros-minsizerel`. NuttX's fixture
profile is `nros_cargo_platform_profile nuttx` → **`nros-minsizerel`**. So the
lane sizes an embedded ROM image with the performance profile.

Measured, same leaf, same revision:

| profile | result |
| --- | --- |
| `nros-relwithdebinfo` (what the lane uses) | overflow **538800** |
| `nros-minsizerel` (what the platform uses) | overflow **419992** |

**Fixed**: `rust-rtos-link-check` now resolves each leaf through
`nros_cargo_platform_profile <platform>` and `nros_cargo_profile_args_for`, the
accessors the fixture lanes already use, and prints the profile it chose. Both
ARM leaves build at `nros-minsizerel`; `threadx-linux` is hosted, has no ROM
region, and keeps the ambient profile — routed through the same accessor so the
three read alike and a future carve-out reaches it without an edit.

Verified: freertos links clean at the size profile, and the NuttX figure this
issue reports is now the platform-true **419992** rather than 538800.

Building what the platform ships also stops this lane writing a SECOND profile
directory beside the fixtures — the builder/probe disagreement phase-340 P2
names as a permanent false-STALE source.

**This does NOT fix the issue.** The size profile still overflows by ~420 KB;
that remainder is the defect the bisect above localises. What the fix buys is a
readable number: the lane now reports the overflow the shipped configuration
actually has.
