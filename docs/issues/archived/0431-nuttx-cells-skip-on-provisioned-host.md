---
id: 431
title: Every NuttX test cell skips on a fully provisioned host — NUTTX_DIR is
  never exported and nothing provisions kconfig
status: resolved
type: bug
area: testing
related: [0407, 0420, 0196]
resolved_in: kconfig venv self-provision
---

Two guards made NuttX cells skip on a host that ran only `nros setup
qemu-arm-nuttx`: `NUTTX_DIR` unset (source-tree-not-found) and the kernel never
configured (no `config.h`, because kconfig was absent).

- **(1) NUTTX_DIR export — already handled.** `scripts/sdk-env.sh` (sourced by
  `activate.sh` after `.env`) evaluates `just --evaluate NUTTX_DIR` from
  `just/sdk-env.just` (`env("NUTTX_DIR", third-party/nuttx/nuttx)`) and exports it.
  Verified in a clean env: `source ./activate.sh` sets
  `NUTTX_DIR=…/third-party/nuttx/nuttx`. The filing's "never exported" was stale
  (predates the phase-218 / issue-0373 sdk-env wiring). A host WITHOUT `just` gets
  a named warning instead, not silence.

- **(2) kconfig provisioning — FIXED.** `nros setup <board>` provisions toolchain,
  qemu and sources but not a kconfig frontend, so `just nuttx build` hard-errored
  "kconfig tools not found" with a `pip install kconfiglib` remedy that PEP-668
  distros (Arch, Debian 12+) refuse. `scripts/nuttx/build-nuttx.sh` now
  SELF-PROVISIONS kconfiglib into a repo-local venv
  (`build/nuttx-kconfig-venv`) when neither `kconfig-conf` nor `olddefconfig` is on
  PATH — a venv's own pip is not blocked by PEP-668, and no sudo is needed. The
  error survives only for the genuinely-broken cases (no python3 / no venv module /
  offline), now naming a distro package rather than a refused `pip install`.
  Verified: `python3 -m venv` + `pip install kconfiglib` yields
  `olddefconfig`/`genconfig`/`menuconfig` on PATH; the happy path (kconfig already
  present) is untouched — the new block is gated on absence.

- **(3) visibility — already present.** `just nuttx doctor` reports the source,
  apps, `config.h` state, cross-compiler and kconfig availability, so an
  unconfigured tree is not silent.

Net: on a fresh / PEP-668 host, `just nuttx build` (hence `build-fixtures`) now
configures the kernel without manual venv+kconfiglib steps, so the cells run
instead of skipping — which is how issue 0420 could assert broken behaviour that
was never broken.
