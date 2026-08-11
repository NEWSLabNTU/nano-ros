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

## Bisect (2026-08-11) — the boundary is phase-338 W2, and #440 only restored HALF

Both ends validated first, per 0477: `87eac17d1` OVERFLOW 538800,
`9748f7ae3` (7 days back) FIT. `git bisect run` over ~600 commits with a probe
that rebuilds the leaf into a FRESH target dir OUTSIDE the repo, so no artifact
staged against another revision's export can survive into a measurement.

```
b393b4737  2026-08-05T21:34  FIT           docs(phase-338): record the W2 decision …
ab486a8db  2026-08-05T21:34  BUILDFAIL     refactor(phase-338 W2): collapse the 18 `-entry` packages
67e823492  2026-08-06        BUILDFAIL     fix(phase-338 W2): the nuttx collapse buried the entry's .cargo…
610ad5bd3  2026-08-06T02:47  OVERFLOW      fix(#440): restore the board's static link args the -entry collapse buried
…all later revisions OVERFLOW, at a CONSTANT 534704 bytes
```

**Reading it.** The last state that both links and fits is the commit
immediately before the `-entry` collapse. The collapse itself broke linking
outright — `undefined reference to open / write / __errno`, which is issue 0440
— so every revision between it and #440's restore is unmeasurable for SIZE, and
the probe skipped them rather than blaming them (55 of 70 probes). #440 made the
image link again, and it has overflowed from that commit onward.

So **#440 restored linkability, not the link configuration**. The `-entry`
packages' link args and the ones restored into the node packages are not
equivalent: the difference costs ~534 KB of ROM. That is where to look — not at
code growth, which is what a 500 KB figure suggests and what the constant
overflow value argues against. A number identical across dozens of revisions is
a switch, not accumulation.

**Caveat on the boundary.** Because the intervening revisions cannot link, the
bisect proves "not before `ab486a8db`, observable from `610ad5bd3`". If the size
change actually arrived somewhere inside that unlinkable window, this bisect
cannot tell — it can only say the first revision where the image both links and
is too big is #440's restore.

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
