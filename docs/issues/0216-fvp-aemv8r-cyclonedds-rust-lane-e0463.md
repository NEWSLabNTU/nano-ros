---
id: 216
title: "`just zephyr build-fvp-aemv8r-cyclonedds-rust` red: E0463 can't find crate `core` for the aarch64 zephyr target"
status: open
type: bug
area: zephyr
related: [phase-291, phase-217]
---

## Problem

The FVP AEMv8-R **Rust** cyclone talker lane fails at the west/cargo build:

```
error[E0463]: can't find crate for `core`
error: could not compile `stable_deref_trait` (lib) due to 1 previous error
error: could not compile `byteorder` (lib) due to 1 previous error
...
FATAL ERROR: command exited with status 101: cmake --build .../build-fvp-aemv8r-cyclonedds-rust-talker
```

Runtime deps for the aarch64 Zephyr target compile without a `core` sysroot —
the lane's rust patching (`scripts/zephyr/aarch64-rust-patch.sh` +
`cortex-a9-rust-patch.sh`) no longer produces a resolvable target/std setup
(rust-src/build-std or an installed target for the aarch64 zephyr triple).

## Baseline-verified pre-existing

Found during phase-291 W4 (the new grep-gate surfaced
`examples/zephyr/rust/cyclonedds/talker-aemv8r` as the 14th bake leaf; its
lane was then exercised for build proof). Verified with the phase-291
migration STASHED: the failure is byte-identical at baseline, so it predates
the migration. The cpp sibling (`build-fvp-aemv8r-cyclonedds`) is unaffected.
The lane is not part of `just zephyr build-fixtures` (native_sim), so no CI
sweep caught the rot.

## Fix direction

Re-run the lane's toolchain assumptions against the current pinned nightly:
whether the aarch64 zephyr target needs `-Z build-std` (json/tier-3 spec, like
the nuttx lanes) or a `rustup target add`, and whether the two rust-patch
scripts still apply cleanly to the pinned zephyr-lang-rust module. Consider
folding the lane into a periodic build sweep once green so it can't rot
silently again.
