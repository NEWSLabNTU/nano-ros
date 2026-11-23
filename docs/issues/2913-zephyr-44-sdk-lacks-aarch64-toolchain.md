---
id: 2913
title: "The Zephyr 4.4 line's SDK is provisioned without aarch64-zephyr-elf, so no 64-bit Arm image (qemu_cortex_a53, FVP) builds on it"
status: open
type: bug
area: [zephyr, toolchain, setup]
severity: medium
found: 2026-11-24
related: [issue-1259, issue-2902]
---

## What happens

On host 241 the 4.4 line's SDK (`~/.nros/sdk/zephyr-sdk-1-0-1/1.0.1/zephyr-sdk-1.0.1`)
holds two toolchains under `gnu/`: `arm-zephyr-eabi` and `x86_64-zephyr-elf`.
It has no `aarch64-zephyr-elf`. So a 64-bit Arm image cannot build on the 4.4
line on this host: the four `qemu_cortex_a53/qemu_cortex_a53/smp` rows in
`examples/fixtures.toml` (the realtime entry, two log-sink cells and the
dead-holder cell) and the FVP (`fvp_baser_aemv8r`) recipes.

The 3.7 line on the same host has it. Its SDK, 0.16.9, is the full bundle, and
`aarch64-zephyr-elf` is one of the ten toolchains it ships. So the same rows
build on 3.7 and not on 4.4, and the line is the only difference.

## Why

`[tool.zephyr-sdk-1-0-1]` in `nros-sdk-index.toml` fetches the `_minimal`
archive. Its `post_install` then fetches only the toolchains it names:

    post_install.run = "./setup.sh -t arm-zephyr-eabi -t x86_64-zephyr-elf -h"

That was the right fix for issue 1259, where the minimal bundle arrived with no
`gnu/` at all. But the target list is the same one `[tool.zephyr-sdk]` (0.16.9)
passes, and there it only registers toolchains the full bundle already holds:
its `post_install.why` says so ("the toolchains are already in this bundle").
On 1.0.x the list is the whole answer, so a toolchain it leaves out is never
installed.

`scripts/zephyr/setup.sh --phase-117` (or `--target aarch64-zephyr-elf`)
exists for this, but it only reaches `install_sdk`, and `install_sdk` runs only
when no SDK is installed yet. On a host that already holds the 4.4 SDK, whether
from an earlier setup or from `nros setup --tool zephyr-sdk-1-0-1`, the flag
fetches nothing and setup reports "Zephyr SDK already installed". Nothing in a
fixture row or board says that it needs the flag, either.

## Two things that make it worse

- **Nothing refuses early.** `nros_zephyr_sdk_env`
  (`scripts/build/zephyr-toolchain.sh`) checks that the tree's pinned SDK
  version is installed, and stops there. It does not check that the SDK holds
  the board's toolchain. So a 4.4 build of an a53 row passes that check and
  fails later, inside Zephyr's toolchain cmake.
- **The FVP recipes probe the 0.16.x layout.** `just/zephyr-setup.just` (the
  FVP ws-entry and board-import recipes) tests
  `$sdk/aarch64-zephyr-elf/bin/aarch64-zephyr-elf-gcc`. On 1.0.x the toolchain
  is under `$sdk/gnu/`, so those recipes would skip on the 4.4 line ("toolchain
  missing") even on a host that had fetched it.

## What is not affected

No merge-gating or nightly lane builds a 64-bit Arm image on the 4.4 line
today. The nightly's 4.4 jobs build `c/talker` (both lines) and the copy-out
check, both on 32-bit or native boards. This is a gap for anyone who selects
the 4.4 line for a whole fixture sweep or for an a53/FVP board.

## Directions (not decided)

1. Add `-t aarch64-zephyr-elf` to the 1.0.1 `post_install` and an aarch64 row
   to its `smoke`. That costs every 4.4 host roughly one more toolchain download.
   Check how `.nros-provenance` records a completed `post_install`: a host that
   already completed the old command may not re-run it.
2. Let a row or board state the toolchain it needs, and have setup fetch what
   the selected line's rows need. That is more work and makes the 3.7 bundle's
   "has everything" an accident nothing relies on.

Either way:

- `nros_zephyr_sdk_env` should check that the board's toolchain is in the
  pinned SDK, and name the fetch command when it is not.
- The FVP probes should ask the SDK's layout rather than hardcode the 0.16.x
  one.

## Closes when

On a host provisioned by `NROS_ZEPHYR_VERSION=4.4 just zephyr setup`, a
qemu_cortex_a53 SMP row builds on the 4.4 line. Or it is refused before
configure, with a message naming the missing toolchain and the command that
fetches it. The FVP recipes find aarch64 under either SDK layout.
