---
id: 2915
title: "A relative NUTTX_DEFCONFIG is read from inside the NuttX tree, and build-nuttx.sh reports the missing file as \"declares no CONFIG_ARCH\""
status: open
type: bug
severity: low
area: [nuttx, build, ux]
found: 2026-11-23
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
