---
id: 1591
title: "`check-stack-floor` has no RISC-V `nm` on the self-hosted runner, so it
  refuses a verdict and takes the whole `esp32` module of the tier-2 build down"
status: open
type: bug
area: ci, tooling, embedded
severity: high
related: [1158, 1346, 1457, 1482, 1344]
---

## What happens

`run-matrix` (tier 2, 1-wise) run **36825211926** (schedule, 2026-10-01T06:31),
job **110249254587**, step 6 `just build tier2`, on the self-hosted
`nano-ros-runner`. The `esp32` module fails, and the decisive lines are its own:

```
  → examples/esp32-c3-baremetal/rust/talker
check-stack-floor: no RISC-V `nm` found (tried riscv32-esp-elf-nm,
  riscv64-unknown-elf-nm, llvm-nm, and
  ~/.espressif/tools/riscv32-esp-elf/*/riscv32-esp-elf/bin/).
Refusing to report a verdict without one — a missing tool is not a pass.
error: recipe `build-qemu` failed with exit code 1
```

The leaf itself built. What failed is the stack-floor check that runs after it,
because none of the four places it looks for a RISC-V `nm` has one on this
machine.

## Why the refusal is right and the failure is still a bug

The refusal is exactly the behaviour this repo asks for — a missing tool is not a
pass, and the alternative (skipping silently) is the vacuous-check class the
pitfall index spends a page on. So nothing about `check-stack-floor` should
change to make this green.

What is wrong is that the tool is absent from the runner at all. Per
[multi-agent-ci-workflow.md](../development/multi-agent-ci-workflow.md), **a
self-hosted runner IS a container, so a missing dependency is an IMAGE fix, never
a host `apt install`**: the running container is `--cap-drop ALL` non-root and
`runner-provision.sh` never sudoes, so the Dockerfile that `runner-container.sh`
generates from `nros-sdk-index.toml` is the only producer of a system package.
That is the same shape as issue **1482**, where the `[python.*]` half of the index
never reached the image, and issue **1457**'s family.

## What this is NOT

- **Not issue 1346.** That was `check-stack-floor`'s own coverage assertion being
  red on `main`, fixed by a `ROW_PLATFORM_BOARD` entry. Here the selftest is not
  the complaint; the probe cannot run because a binary is missing.
- **Not issue 1356.** That is the `esp32` nightly failing on offline registry
  resolution (`esp-backtrace`). The `esp32` job of the 07:19 nightly was **green**
  on this same day (run 36829686786) — this failure is on the tier-2 lane and a
  different runner.
- Not a nano-ros build defect at all: the image under test compiled.

## What would close it

1. Decide which RISC-V `nm` the esp32 coordinate is entitled to, and add it to
   the runner image through the index that generates the Dockerfile — not by hand
   on the box, which the container cannot keep.
2. Measure it inside the image, not on the machine that built it (the apt-vs-pip
   distinction in the workflow doc).
3. Acceptance is the `esp32` module of a tier-2 build reaching a verdict on this
   runner, green or red on the stack floor itself rather than on the absence of a
   tool.

Related, and not fixed here: issue 1346's "Not fixed here" note says building one
esp32 image is nightly-only and that the lane exemption is one problem with one
fix. This failure is the tier-2 lane finding what the nightly's own esp32 job
does not, which is evidence for that note rather than against it.
