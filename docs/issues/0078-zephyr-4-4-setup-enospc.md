---
id: 78
title: nightly zephyr 4.4 cells fail — ENOSPC during `just zephyr setup` pip install
status: open
type: bug
area: zephyr
related: [phase-253, phase-180]
---

## Problem

Several `zephyr 4.4 / *` cells in the `nightly` lane fail at the
`Set up Zephyr 4.4 workspace` step (`just zephyr setup --skip-sdk`). Root
cause is disk exhaustion, not code:

```
Installing build dependencies: finished with status 'error'
  error: subprocess-exited-with-error
  ERROR: Could not install packages due to an OSError:
         [Errno 28] No space left on device
ERROR: Failed to build 'spsdk-mcu-link' when installing build dependencies for spsdk-mcu-link
error: recipe `setup` failed with exit code 1
```

The 4.4 west manifest (`west-4.4.yml`) pulls a heavier dependency set than the
3.7 line — `west update` checks out more modules and the requirements pip-build
(`spsdk-mcu-link` and friends) fills the container's ~14 GB host disk. The 3.7
cells pass; only 4.4 cells trip it.

## Evidence

- CI: `nightly` workflow, run 27662734542 (2026-06-17, build-only dispatch).
  Failing cells all stop at `Set up Zephyr 4.4 workspace`:
  `zephyr 4.4 / {rust/talker, rust/listener, c/talker, cpp/listener}`.
  The 3.7 matrix + all 6 platform cells + `zephyr ci-both` + `copy-out` pass.
- The merge structure is NOT implicated: every step before setup
  (checkout / CLI build / source provisioning / SDK register / clippy unblock)
  succeeds; the 3.7 cells run the identical structure green.

## Root cause (investigated 2026-06-18)

`scripts/zephyr/setup.sh` (line ~433) installed the FULL
`zephyr/scripts/requirements.txt`, which `-r`s Zephyr's
extras/run-test/compliance sub-requirements. On the 4.4 line the extras set
pulls `spsdk-mcu-link` (NXP MCU flash/sign tooling; heavy crypto +
build-isolation deps) — its pip build fills the ~14 GB container → ENOSPC. 3.7's
older requirements.txt has no spsdk, so 3.7 cells pass. No version branching: a
single unconditional `pip install -r requirements.txt` for both lines.

The nano-ros zephyr flows are QEMU **build-only** (`west build`):
`requirements-base.txt` (pyelftools/packaging/pykwalify/anytree/intelhex/
devicetree) is sufficient; the extras (flashing/twister/compliance) are never
exercised.

## Fix (Option C — base-only requirements + disk-reclaim)

Two parts:

1. **base-only requirements** (`setup.sh`): install `requirements-base.txt`
   (fallback to full only if base absent) instead of the full `requirements.txt`.
   Drops `spsdk-mcu-link` + the unused extras/run-test/compliance sets.

2. **disk-reclaim** (nightly.yml, 3 zephyr jobs): the first dispatch showed A
   alone took 7/9 4.4 cells green but 2 still tipped (`Free space left: 78 MB`
   → `[Errno 28]`) — Zephyr **4.4's own `requirements-base.txt` still lists
   `reuse` + compliance deps** (line 27: `reuse>=6.0.0` → Jinja2/click/
   license-expression/python-debian/python-magic/tomlkit), so base isn't lean
   on 4.4 and the ~14 GB disk stays borderline. Added a reclaim step before
   `just zephyr setup` (apt clean + drop `/usr/share/{doc,man,locale}` +
   `~/.cache/pip`; the baked SDK stays) — mirrors the `platform` job.

A drops the biggest hog; the reclaim covers the remaining margin.

Verify: re-dispatch `nightly` build-only — all 4.4 cells clear
`Set up Zephyr 4.4 workspace`.
