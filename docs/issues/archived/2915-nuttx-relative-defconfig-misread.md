---
id: 2915
title: "A relative NUTTX_DEFCONFIG is read from inside the NuttX tree, and build-nuttx.sh reports the missing file as \"declares no CONFIG_ARCH\""
status: resolved
type: bug
severity: low
area: [nuttx, build, ux]
found: 2026-11-23
resolved: 2026-11-24
related: [2914]
---

**Owner:** unassigned.

## What happens

`scripts/nuttx/build-nuttx.sh` stores `NUTTX_DEFCONFIG` as given, then
`cd "$NUTTX_DIR"` before its first read. A path relative to the checkout
(`packages/boards/nros-board-nuttx-qemu/nuttx-config/riscv/defconfig`, what a
person types from the repo root) is then resolved under the NuttX tree, where
it does not exist. The read is
`grep -E '^CONFIG_ARCH=' "$DEFCONFIG" 2>/dev/null`, so the missing file is
silenced and the script stops with:

```
build-nuttx.sh: packages/.../riscv/defconfig declares no CONFIG_ARCH — cannot key the export
```

That sends the reader into a defconfig that is fine. Every later
`2>/dev/null` read of `$DEFCONFIG` (hash, `CONFIG_ARCH_BOARD`) has the same
blind spot.

## Fix

Resolve `DEFCONFIG` to an absolute path against the caller's cwd before the
`cd` (`realpath -e`), and refuse a path that does not exist, naming the path as
given and as resolved. Then the `2>/dev/null` reads can only meet a file that
exists. Acceptance: a relative path from the repo root builds; a missing one
fails naming the file.

## Resolution

Fixed 2026-11-24. `build-nuttx.sh` resolves `NUTTX_DEFCONFIG` against the caller's cwd before any `cd` (`nros_nuttx_resolve_defconfig` in `scripts/nuttx/build-inputs.sh`) and refuses a path that is not a file, naming it as given and as resolved, so the `CONFIG_ARCH` read only meets a file that exists. Gate: `just check nuttx-build-inputs`, whose negative control is the same relative path resolved from inside a NuttX tree.
