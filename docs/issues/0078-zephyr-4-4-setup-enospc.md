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

## Direction

Add a disk-reclaim step to the nightly Zephyr jobs BEFORE
`just zephyr setup` — mirror the `platform` job's "Reclaim disk before build"
(`apt-get clean` + `rm -rf /var/lib/apt/lists/* /usr/share/{doc,man,locale}`).
Alternatively prune the 4.4 west import set / skip the `spsdk-*` flashing tools
the nano-ros examples don't need (a west manifest `import` filter), or set
`PIP_NO_BUILD_ISOLATION`/a tmp on a larger mount. Confirm by re-dispatching
`nightly` build-only and checking the 4.4 cells reach `build-one`.
