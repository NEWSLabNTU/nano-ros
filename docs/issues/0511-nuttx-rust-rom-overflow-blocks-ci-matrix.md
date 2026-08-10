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

## Not yet done

No bisect. The two ends were validated (the failure reproduces with and without
the newest change, from a clean tree), which is what 0477's own write-up says to
do before stepping — but the actual first-bad commit has not been found.
