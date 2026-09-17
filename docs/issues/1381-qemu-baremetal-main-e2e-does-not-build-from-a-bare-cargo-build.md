---
id: 1381
title: "`qemu-baremetal-main-e2e` cannot be built with `cargo build` after
  `nros sync` — `nros::main!()` emits a `std` path into a `#![no_std]` leaf"
status: open
type: bug
area: [build, codegen, testing]
severity: medium
found: 2026-09-17
related: [issue-1061, issue-0100]
---

## What happens

In a clean worktree with the CLI built:

```
cd packages/testing/nros-tests/bins/qemu-baremetal-main-e2e
nros sync
cargo build --release
```

```
error[E0433]: cannot find `std` in the crate root
  --> src/main.rs:14:1
error[E0433]: cannot find `std` in the crate root
  --> src/main.rs:14:1
error: could not compile `qemu-baremetal-main-e2e` (bin) due to 2 previous errors
```

Line 14 is `nros::main!(panic = "own");` in a leaf whose first two attributes
are `#![no_std]` / `#![no_main]`. So the expansion names `std::` twice on a
target that has none — CLAUDE.md's "`std` is being DELETED; write `core::`/
`alloc::`, never `std::`" rule, in generated code.

## The precondition this probably rides on

`nros sync` in that leaf does not complete cleanly either:

```
sync: source metadata — no producer for
  qemu_baremetal_main_e2e::qemu_baremetal_e2e
  (deploy-bound probe failed: metadata-mode harness failed (exit 101) for
   component 'qemu_baremetal_e2e': error: invalid instruction mnemonic 'bkpt')
sync: 1 component(s) are un-probeable, so pool budgets stay at the crate
  defaults (issue 1061)
```

The metadata-mode harness builds the component for the HOST to probe it, and a
Cortex-M leaf's `bkpt` does not assemble there. Issue 1061 already covers the
degradation ("budgets stay at the crate defaults"). Whether the `std` paths are
a SECOND defect or a consequence of the un-probeable path taking a different
expansion branch is **not established** — both were observed together and only
once.

## Why it matters

This fixture is the canonical bare-metal `nros::main!()` BoardEntry E2E image,
and it is the natural stand-in whenever someone needs a real 32-bit Cortex-M
ELF that links both `nros-rmw-zenoh` and a board's rlsf arena. The phase-392
amendment B measurement wanted exactly that and had to fall back to a
`thumbv7m-none-eabi` object probe for the allocator half.

`just qemu build-fixtures` presumably supplies whatever the bare `cargo build`
is missing, so the lane is green and the leaf is unbuildable by hand — which is
the gap worth closing either way: a standalone copy-out fixture that only
builds under one recipe is not standalone.

## What to establish first

1. Does `just qemu build-fixtures` build it today, and what does it pass that a
   bare `cargo build` does not?
2. Expand the macro (`cargo expand`, or the entry-lower output) and name the
   two `std::` paths. If they are unconditional, it is a codegen bug
   independent of 1061.
