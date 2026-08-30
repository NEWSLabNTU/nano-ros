---
id: 937
title: "A generated Entry names only its platform feature, so a `no_std` platform
  with no other allocator provider cannot link — nuttx has been red every nightly"
status: open
type: bug
area: codegen
related: [0594, 0616]
---

## What is wrong

`packages/cli/nros-cli-core/src/builder/entry.rs:344` writes the Entry's
dependency on `nros-platform` with exactly one feature — the board's
`platform_feature`:

```rust
out.push_str(&format!(
    "nros-platform = {{ path = \"{plat_rel}\", default-features = false, \
     features = [\"{}\"] }}\n",
    board.platform_feature
));
```

`#[global_allocator]` in `nros-platform` is **opt-in**, behind the
`global-allocator` feature, and deliberately so — `lib.rs:133` says *"Off by
default — `platform-posix` users link against libstd's allocator"*. Since
phase-361 W8.c / issue 0594 it is the ONE `#[global_allocator]` in the tree.

A generated Entry therefore never enables it. On any `no_std` platform where
nothing else supplies one, the build ends at:

```
error: no global memory allocator found but one is required; link to std or
       add `#[global_allocator]` to a static item that implements the
       GlobalAlloc trait
error: could not compile `nuttx_entry` (bin "nuttx_entry") due to 1 previous error
```

That is the nightly `nuttx` cell, on
`examples/workspaces/realtime-rust/build/nuttx-zenoh/nuttx_entry`. It has been
red for every run in the scanned window.

## Why only nuttx

The feature is enabled in exactly six places in the tree, all HAND-WRITTEN
examples on one platform:

```
examples/qemu-riscv64-threadx/rust/{talker,listener,service-*,action-*}/Cargo.toml:
  nros-platform = { ..., features = ["platform-threadx", "global-allocator", "critical-section"] }
```

No generated Entry has ever carried it. Other platforms survive because
something else in their graph provides an allocator — Zephyr's Rust images get
one from `zephyr-lang-rust`, POSIX from libstd. NuttX has no such provider, so
it is the platform where the omission becomes a link error rather than a latent
gap. Any future `no_std` platform without a provider inherits the same failure.

## The shape of the fix

Do NOT hardcode a platform list in the generator. The Entry builder already
takes everything platform-specific from the board descriptor —
`platform_feature` (`entry.rs:107`) and `crate_root_deps` (`entry.rs:113`), the
latter existing precisely because *"the descriptor is the only place that
knows"*. Whether a platform supplies its own allocator is the same kind of fact.

So: give the descriptor a field for the extra `nros-platform` features an Entry
needs on that board (or a narrower `needs_global_allocator` flag), default it
off, set it for the NuttX boards, and have `entry.rs` emit it alongside
`platform_feature`.

The comment two lines above the offending code already states the principle this
violates — the platform feature is named explicitly because *"an unselected
platform is a link error a long way from its cause"*. An unselected allocator is
the same error from the same cause, and was left out.

## Verification

`nuttx` is one of five cells `just nightly-triage` reports as red across every
run in the window. The other four are already fixed and awaiting a nightly:
`esp32` by #89 (DRAM overflow — service buffers for services the image lacks),
and `freertos` / `threadx_linux` / `threadx_riscv64` by #90, which share ONE
cause — `idlc not found on PATH`, the Cyclone host tool that `nros setup` never
provisioned because `--rmw` defaulted to zenoh. Both merged at ~14:04 and
~14:20 UTC on 2026-08-30, after the 07:10 run that produced the failures above.

Note the two shapes the three `idlc` cells took, because they read completely
differently in the run list: `freertos` died at cmake configure with the tool
named, while both `threadx` cells built, then SKIPPED all 9 cyclone tests for
unmet preconditions and went red on `_check-skip-budget` reporting
`Real failures: 0 / 0 total failures`. Same root cause, one visible and one
laundered into a skip-budget red.
