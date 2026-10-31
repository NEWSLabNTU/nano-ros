---
id: 2113
title: "The Zephyr C core-pin placement cell boots `qemu-system-aarch64`, which the runner does not have, and fails with a bare ENOENT"
status: open
type: bug
area: [ci, testing, zephyr, qemu]
severity: medium
found: 2026-10-27
related: [1991, 0750, 0930, phase-513, phase-466]
---

## Measured

- Run: nightly 40466371508 (schedule, 07:13 UTC, main at `5e863e1ee`).
- Job: 122062645403, `tier 2 nightly (pairwise cover)`.
- Step: `just ci matrix-nightly`.

`nros-tests::sched_dims_applied_e2e sched_dims_applied` is one of the job's three real failures (`Real failures: 3 / 3 total failures`):

```
sched_dims: 1 of 17 cell(s) FAILED:
  CorePinPlacement/zephyr/c: [zephyr c] boot zephyr a53 smp: Process failed to start: No such file or directory (os error 2)
```

The other 16 cells passed.

## Reading

`Boot::ZephyrQemuA53Smp` calls `ZephyrProcess::start(.., ZephyrPlatform::QemuCortexA53Smp)`. That spawns `crate::qemu::qemu_system_cmd("qemu-system-aarch64")`, which resolves through `qemu_system_path` in this order:

1. `QEMU_SYSTEM_AARCH64`.
2. The project prefix.
3. The store row `[tool.qemu]`.
4. The bare name on `PATH`.

`[tool.qemu]` in `nros-sdk-index.toml` is configured with `--target-list=arm-softmmu,riscv64-softmmu`, so the store copy holds no `qemu-system-aarch64`. Nothing else in `.github/`, `scripts/` or `just/` provisions one. The resolver therefore fell through to the bare name, and the spawn failed with ENOENT.

That is an inference from the code and the index. The runner image was NOT inspected.

There are two separate defects here:

1. **No emulator.** The tier-2 nightly lane selects this cell, but no provisioning path supplies the binary it boots.
2. **No precondition.** The cell never asks `qemu_system_available("qemu-system-aarch64")` before booting. The missing tool therefore surfaces as a spawn error, not as an `[UNMET PRECONDITION]` naming the emulator. (The `QemuArm` siblings go through `qemu_system_arm_cmd`, whose binary the store does hold.)

## What it is NOT

- **Not a fixture build failure.** The ELF was resolved, and the failure is at process spawn.
- **Not issue 0750** (archived), which made the SMP core-pin arms run at all.
- **Not a regression found by bisect.** The cell dates from `e33064c64f` (2026-08-20). The tier-2 nightlies of 2026-10-24 to 2026-10-26 stopped in the fixture build (issue 1991), so this is the first night since then that reached the cells. When it first went red is NOT established.

## What would close it

- **Provide the emulator.** Either add `aarch64-softmmu` to `[tool.qemu]`'s `--target-list` (with a smoke row for `bin/qemu-system-aarch64`), or provision it from the distribution package in the runner image (a runner is a container, so this is an image fix and not a host install).
- **Add the precondition.** Before the a53 SMP boot, check `qemu_system_has_machine("qemu-system-aarch64", "virt")`, so a host without it reports which tool is missing.
- **Acceptance:** `CorePinPlacement/zephyr/c` passes in a tier-2 nightly.

## Fixed in the tree, 2026-10-29 (still open)

Both halves are in, and the issue stays open until the acceptance nightly.

- **The emulator.** `nros-sdk-index.toml` has a new row,
  `[prereq.qemu-system-aarch64]`: on apt it is `qemu-system-arm`, the same
  package as the 32-bit emulator, and on dnf and pacman it is
  `qemu-system-aarch64`. `scripts/ci/runner-container.sh` adds it to
  `PREREQ_KEYS`, so the generated runner Dockerfile installs it. The
  distribution package was chosen over widening `[tool.qemu]`'s
  `--target-list`. Widening it would rebuild the store QEMU on every host for
  one cell, and the `virt` machine with two A53s is in every packaged QEMU this
  index supports. **The runner image has to be rebuilt by its operator**
  (`scripts/ci/runner-container.sh --build`, then `--run`). Until then the
  nightly runs the old image and the cell still fails, now with the message
  below.
- **The precondition.** `nros_tests::qemu::is_qemu_aarch64_virt_available()`
  asks `qemu_system_has_machine("qemu-system-aarch64", "virt")`. The
  `Boot::ZephyrQemuA53Smp` arm in `sched_dims_applied_e2e.rs` calls it before
  `ZephyrProcess::start`, and when it fails it reports `nros_tests::unmet!`
  naming the emulator, the store build's missing target and the prereq. The
  check sits in the test and not in `ZephyrProcess::start`: placed there, it
  sat beside that function's fixture-existence guard, and
  `check-fixture-require` read it as one more existence-guarded skip. Inside `sched_dims`, an unmet
  precondition still counts as a failure (issue 1758), so the cell stays red
  on a runner without the emulator. The difference is that the red now names
  the tool.

**Acceptance is unchanged:** `CorePinPlacement/zephyr/c` passes in a tier-2
nightly that runs on a rebuilt runner image.

## Measured on the old image, 2026-10-31 (still open)

Tier-2 nightly 41018247377 (2026-10-30, main at `1c81f56b75`) reached the
cells, and this cell was its only failure (`failures: 1 / 1 total failures`).
It now fails with the precondition message, not a bare ENOENT:

```
CorePinPlacement/zephyr/c: [UNMET PRECONDITION] [zephyr c] qemu-system-aarch64 with the `virt` machine is not available, so the Zephyr qemu_cortex_a53 SMP image cannot boot. ...
```

So the precondition half is confirmed in CI. The emulator half waits on the
runner image: the generated Dockerfile installs `qemu-system-arm` (which
carries `qemu-system-aarch64` on apt), and the image has not been rebuilt
since. The local `nano-ros-runner:local` dates from 2026-09-25. The
maintainer rebuilds it (`scripts/ci/runner-container.sh <labels> --build`,
then `--run`). After that, the next tier-2 nightly that reaches the cells is
the acceptance run.

Two other things keep a nightly from reaching the cells, and neither is this
issue:

* nightly 41179280379 (2026-10-31) stopped at `check-cmake-workspace-system-required`;
* the tier-2 `run-matrix` dispatches of 2026-10-31 stopped in the Zephyr
  fixture build, at `build-cortex-m-c-boot-report-zenoh`'s configure
  ("Aborting due to Kconfig warnings").
