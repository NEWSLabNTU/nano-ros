---
id: 608
title: "A shared-group fixture row resolves at the AMBIENT cargo profile, so
  every NuttX Rust row is looked up under `nros-relwithdebinfo` while the
  builder writes `nros-minsizerel`"
status: resolved
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

## Resolution 2026-08-16

Fixed as the sketch asks — one derivation, applied at the chokepoint.

**`nros_cargo_profile::platform_profile(platform)`** is the Rust twin of
`nros_cargo_platform_profile` in `scripts/build/cargo.sh`, keyed on the same
coordinate `platform` values the manifest emits (`freertos`, `nuttx`,
`nuttx-riscv`). `CARVE_OUTS` could not serve: it is keyed by carve-out NAME
(`nuttx-rust`), which no resolver holds — what a resolver has is the row's
coordinate. That mismatch is why each site rebuilt the mapping itself and the
group-row resolver never did.

**The rewrite happens in `require_prebuilt_row_binary`**, not at the nine call
sites that build `rel` from the ambient profile. Same reasoning the neighbouring
`require_prebuilt_binary` already records for its own redirect: the funnels are
not the whole class, and fixing a subset of resolvers is the #328 shape. Only
the path COMPONENT equal to the ambient profile dir is replaced, so a binary or
triple that happens to share the name is left alone.

`freertos-qemu` — which this issue flagged as a candidate for the same failure —
is covered by the same change rather than a second fix.

### Both guards were mutation-tested, and the first one was inadequate

* `platform_profile_agrees_with_the_shell_builder` PARSES `cargo.sh`'s switch
  and asserts the Rust table answers the same for every arm. Restating the
  constants would have agreed with itself forever. Verified by deleting the
  `"nuttx"` arm: fails with *"cargo.sh maps platform "nuttx" to
  nros_cargo_nuttx_profile; platform_profile disagrees"*.

* `a_row_is_resolved_at_its_platforms_profile` covers the rewrite itself.
  **It passed with the chokepoint bypassed** — it exercises the helper, not the
  wiring, which is exactly the issue-0196 gap this issue is an instance of. So
  `the_row_resolver_uses_the_carve_out_profile` drives the real resolver and
  asserts on the path it went looking for; that one does fail when the call is
  removed.

  It reads a PANIC rather than an `Err`: a missing in-lane fixture is issue
  0584's "broken promise", not a recoverable error. The first cut used
  `expect_err` and never reached its assertions.

### Build side unchanged, deliberately

`nros_cargo_platform_profile` was already correct — the builder has always
written `nros-minsizerel`. Only the test-side locator moved, so #393's
"move both in the same commit" is satisfied by there being one side to move.
