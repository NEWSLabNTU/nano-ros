---
id: 1771
title: "The mps2/an385 Zephyr Rust talker panics at boot in `executor::storage` carve: an EMPTY parameter-slot slice built from an unaligned pointer"
status: resolved
type: bug
area: [zephyr, core, executor]
severity: high
found: 2026-10-09
resolved: 2026-10-10
related: [phase-382, phase-481]
---

## Measured

`origin/main` `2b81536214`, distrobox `ros2`, Zephyr 3.7, fixture
`build-cortex-m-rust-talker-zenoh` built pristine through `just zephyr
build-fixtures`, then
`cargo nextest run -p nros-tests --test zephyr_cortex_m_qemu -j1 --retries 0
zephyr_cortex_m_rust_zenoh_pubsub_e2e`:

```
panic: panicked at packages/core/nros-node/src/executor/storage.rs:739:24:
unsafe precondition(s) violated: slice::from_raw_parts_mut requires the pointer
to be aligned and non-null, and the total size of the slice not to exceed `isize::MAX`
>>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
```

The talker publishes nothing and the cell fails. The native_sim Rust talker
(x86_64) and the C/C++ cortex-m talkers pass in the same run.

## Cause

Line 739 is the parameter-slot slice phase-382 W3' (`dce93d407f`, landed
2026-10-09) added to `carve`:

```rust
let params_s = core::slice::from_raw_parts_mut(
    base.add(o.params) as *mut MaybeUninit<nros_params::ParameterSlot>,
    param_slots,
);
```

This image builds no parameter store (`NROS_CAPABILITY_PARAM_SERVICES` and
`NROS_PARAM_STORE` both resolve 0), so `param_slots` is 0. But
`from_raw_parts_mut` requires an ALIGNED pointer even for a zero-length slice,
and on thumbv7m `base + o.params` is evidently not aligned to `ParameterSlot`
when no store was laid out. The debug-assertion UB check fires on that target
only; x86_64's layout happens to align.

## Not caused by phase-481 W3

Measured on `origin/main` itself, without the W3 branch. W3's build of the same
image has a `.config` identical to the pre-migration merge, and it panics the
same way.

## What would close it

Make the empty case alignment-correct: lay `o.params` out at
`align_of::<ParameterSlot>()` whether or not the store is present, or use
`NonNull::dangling()` when `param_slots == 0`. Then
`zephyr_cortex_m_rust_zenoh_pubsub_e2e` should pass again.

## Fix

Every slice `carve` builds now goes through one helper, `region_slice`, which
returns `&mut []` for a zero-length region and calls `from_raw_parts_mut`
only for a non-empty one. The empty region needs no address, so the
misaligned offset is never turned into a slice. Fixed as a class, not at the
one site: all eleven slices in `carve` (and the `carved!` `CarvedVec`s) use
it. The layout is unchanged — `nros_executor_layout::offsets` still leaves an
empty region's `off` unmoved, so no backing grows and no C/C++ sizes header
moves; its comment now says an empty region's offset may be misaligned.

## Proof (2026-10-10)

- **The misalignment depends on the image's knobs, so one image is not
  evidence.** The `rust/talker` mps2 image built from `main` in a clean
  checkout BOOTED with the old line (its counts happened to align
  `o.params`), while box2's build of the same leaf panicked at
  `storage.rs:739:24` — the run that filed this. Swapping box2's ELF into the
  passing checkout reproduced the panic under the real test; the checkout's
  own ELF passed it.
- So the regression test is on the host, over the layouts rather than one
  config: `carve_survives_an_empty_parameter_region_at_any_alignment` sweeps
  `cbs` 1..=16 with `params = 0`, requires that the sweep REACHES a misaligned
  offset (else it has stopped testing this), and carves each. Negative
  control: with the old `from_raw_parts_mut` line it aborts with this issue's
  exact message (`SIGABRT`); with the fix it passes. Plus
  `an_empty_region_at_a_misaligned_offset_carves_to_an_empty_slice` on the
  helper itself.
- `zephyr_cortex_m_rust_zenoh_pubsub_e2e` passes with the fix (mps2/an385,
  QEMU, `Publishing: 'Hello World: 1'`).
