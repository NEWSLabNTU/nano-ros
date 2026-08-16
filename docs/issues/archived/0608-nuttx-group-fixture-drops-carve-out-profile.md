---
id: 608
title: "A shared-group fixture row resolves at the AMBIENT cargo profile, so
  every NuttX Rust row is looked up under `nros-relwithdebinfo` while the
  builder writes `nros-minsizerel`"
status: open
type: bug
area: testing
related: [issue-0196, issue-0393, issue-0488, issue-0584, phase-340]
---

## Symptom

`nuttx_entry_demos_build` fails with the issue-0584 "broken promise" panic even
though the fixture build just finished green:

```
Test fixture binary MISSING for an in-lane coordinate:
  build/cargo-fixtures/nuttx-2162892711/armv7a-nuttx-eabihf/nros-relwithdebinfo/listener
```

The binary exists. It is one directory over:

```
build/cargo-fixtures/nuttx-2162892711/armv7a-nuttx-eabihf/nros-minsizerel/listener
```

Same feature-signature group, same target triple, different PROFILE directory.

## Root cause

NuttX Rust has a profile CARVE-OUT. `just/nuttx.just` pins its Rust fixtures to
`nros-minsizerel` (`nros_cargo_platform_profile nuttx` ->
`nros_cargo_nuttx_profile` -> `NUTTX_RUST_PROFILE`), because at `lto=off` a
non-deterministic `armv7a-nuttx-eabihf` cross-CGU bug corrupted the image.

The test side knows this. `fixtures/binaries/nuttx.rs::require_entry_binary`
resolves `nros_cargo_profile::NUTTX_RUST_PROFILE` first and only falls back to
the ambient profile — its own comment says "prefer the profile the builder
forces".

But that carve-out is applied to the LEAF path, and a row built into a
phase-340 shared cargo group does not live in its leaf. The group lookup is a
separate branch in `fixtures/binaries/mod.rs`:

```rust
if crate::fixtures::groups::leaf_has_rows(&leaf) {
    let row = crate::fixtures::groups::select_row(...)?;
    let rel = PathBuf::from(format!("{}/{}", cargo_target_profile_dir(), binary_name));
    return require_prebuilt_row_binary_fresh(row, &rel);
}
```

`cargo_target_profile_dir()` is the AMBIENT profile, unconditionally. So the
carve-out the caller carefully resolved is discarded the moment the row turns
out to be group-built, and every group-built NuttX Rust row is looked up under a
profile directory the builder never writes.

The leaf fallback cannot save it either: the builder writes only to the group,
so the leaf's `target/armv7a-nuttx-eabihf/` holds no `nros-minsizerel` at all
(observed: only stale `release` / `nros-fast-release` dirs).

## Why this is the issue-0196 shape again

CLAUDE.md already states the rule this breaks — "A PLATFORM's fixture profile is
`nros_cargo_platform_profile` — the staleness probe must use it too, or it
rebuilds into a second profile dir and reports permanent false-STALE". The
carve-out was threaded through the leaf resolver and the staleness probe, and
NOT through the group-row resolver that phase-340 added beside them. A third
consumer of a rule that already had two.

`freertos-qemu` has the identical carve-out (`FREERTOS_QEMU_PROFILE`, also
`MINSIZEREL`) and the identical leaf-side handling, so it is a candidate for the
same failure wherever its rows are group-built — worth checking as part of the
fix rather than fixing the NuttX site alone.

## Not caused by phase-359 W7

Found while verifying W7 (NuttX off `std`), but every file involved is unchanged
by it: `fixtures/binaries/mod.rs`, `fixtures/binaries/nuttx.rs`, `just/nuttx.just`
and `nros-cargo-profile/src/lib.rs` are all byte-identical to `HEAD`. W7 changed
the feature signature (hence a new group directory name), which is why it
surfaced here, but both sides of the mismatch predate it.

## Fix sketch

Resolve the profile from the ROW's platform rather than the ambient setting —
the row already carries its coordinate (`row_coord()`), and
`nros_cargo_profile::carve_out(<platform>)` is the existing lookup. One
derivation, used by builder, probe and resolver alike, instead of a carve-out
that each consumer has to remember to apply.

Guard it the way #393 asks: move the build-side and test-side locators in the
same commit, and assert the agreement rather than restating the constant.
