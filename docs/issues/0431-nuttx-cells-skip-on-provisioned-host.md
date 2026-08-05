---
id: 431
title: Every NuttX test cell skips on a fully provisioned host — NUTTX_DIR is
  never exported and nothing provisions kconfig
status: open
type: bug
area: testing
related: [0407, 0420, 0196]
---

## Problem

`nros_tests::fixtures::binaries::nuttx::is_nuttx_available()` gates on
`$NUTTX_DIR/Makefile`, and `is_nuttx_configured()` on
`$NUTTX_DIR/include/nuttx/config.h`. Both are false on a host that has done
everything the docs ask:

- **`NUTTX_DIR` is never exported.** `activate.sh` does not set it, and neither
  does the SDK env; only `just/nuttx.just` computes it locally
  (`nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"`) for its own
  recipes. So the sources can be present and provisioned
  (`nros setup qemu-arm-nuttx` succeeds) while every cell reports
  `[SKIPPED] NuttX source tree not found`.
- **Nothing provisions kconfig.** With `NUTTX_DIR` set by hand, the cells then
  report `[SKIPPED] NuttX not configured`, and `just nuttx build` fails:

  ```
  ERROR: kconfig tools not found (kconfig-conf or kconfiglib).
  ```

  `nros setup qemu-arm-nuttx` provisions the toolchain, QEMU and the sources —
  but not the tool needed to configure the kernel. On a PEP-668 distro
  (Arch, Debian 12+) `pip install kconfiglib` is refused, so the remedy the
  error prints does not work either; a venv or the distro package is required.

The result: `logging_smoke_nuttx_qemu_arm` and the other NuttX cells skip on a
machine that has the sources, the cross toolchain and qemu-system-arm installed.
Nobody sees a red, and nobody sees coverage either.

## Why it matters

Issue 0420 claimed the `nros_log` facade was a silent no-op on NuttX. Deciding
that took building NuttX by hand (venv + kconfiglib + `just nuttx build` +
`just nuttx build-fixtures`) — after which the cell PASSES and the claim is
disproved. A cell that cannot run is worse than a red one: it produced an issue
asserting broken behaviour that was never broken, and it would equally hide a
real regression.

Same family as 0407 (tier 1 selecting a test whose fixture its lane never
builds), one layer further out: there the fixture was missing, here the whole
platform is.

## Reproduction

```sh
nros setup qemu-arm-nuttx          # succeeds
source ./activate.sh
cargo test -p nros-tests --test logging_smoke logging_smoke_nuttx_qemu_arm
# [SKIPPED] NuttX source tree not found
```

## Fix directions

1. **Export `NUTTX_DIR`** from the SDK env / `activate.sh` when the submodule is
   present, the way other provisioned trees are wired. One line, and every
   guard starts telling the truth.
2. **Provision kconfig with the board.** Add it to the `qemu-arm-nuttx` /
   `qemu-riscv-nuttx` package sets, or have `just nuttx build` create the venv
   it needs. Printing `pip install kconfiglib` as the remedy is wrong on any
   PEP-668 distro.
3. **Make an unrunnable platform visible.** A skip that can never turn into a
   pass on any developer machine is indistinguishable from coverage. Once (1)
   and (2) land the cells run; until then, `just doctor` should report NuttX as
   unconfigured rather than staying quiet.

## Notes

Found 2026-08-05 while trying to confirm or refute issue 0420. The NuttX cell
had, as far as this host was concerned, never run.
