---
id: 2113
title: "The Zephyr C core-pin placement cell boots `qemu-system-aarch64`, which the runner does not have, and fails with a bare ENOENT"
status: open
type: bug
area: [ci, testing, zephyr, qemu]
severity: medium
found: 2026-10-27
related: [1991, 0750, 0930]
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
