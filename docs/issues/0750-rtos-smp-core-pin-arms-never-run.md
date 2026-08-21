---
id: 750
title: "the NuttX / FreeRTOS / ThreadX core-pin arms compile but have never RUN — each blocked on a different thing, and only one is close"
status: open
type: limitation
area: testing
related: [issue-0260, issue-0743, issue-0655, phase-356, phase-296]
---

## What this carries over

[issue 0260](archived/0260-native-dim-kernel-accept-never-exercised.md) closed
2026-08-21 on both halves of its narrowed Direction: every core-pin arm now
COMPILES (`just check-sched-dim-arms` type-checks the three RTOS call sites
against their own vendored headers), and one arm RUNS and is OBSERVED on a real
multi-core image —

```
sched-dim arm: [zephyr c CorePinPlacement] ACCEPT
```

on `qemu_cortex_a53/qemu_cortex_a53/smp`, 2 CPUs, asserting `running_on=1`.

This issue is the residual 0260 explicitly declined to carry: **NuttX, FreeRTOS
and ThreadX core-pin arms have still never executed.** Their images are
uniprocessor, so those cells skip. That is board-enablement work, not a
scheduling-dim question, which is why it is its own issue.

## The three are NOT equally blocked

The blanket phrase "three multi-core board bring-ups" is what 0260 left behind,
and it is too coarse to plan from. Measured 2026-08-21:

| RTOS | SMP port available in-tree? | real obstacle |
| --- | --- | --- |
| NuttX | **yes** — `boards/arm/qemu/qemu-armv7a/configs/smp` and `knsh_smp`, plus arm64 and rv-virt variants | the SHARED kernel tree, not the port |
| ThreadX | yes — `ports_smp/cortex_a5_smp` is vendored (the compile gate already type-checks against it) | our ThreadX lane is a **Linux simulator**, not that port |
| FreeRTOS | **no** — see below | no SMP port exists for anything we emulate |

### NuttX — closest, and blocked on the build tree rather than the kernel

The board family the nuttx-arm fixtures already use ships an SMP config. The
obstacle is that NuttX here has ONE build tree with ONE configuration at a time:
`scripts/nuttx/build-nuttx.sh` reconfigures `$NUTTX_DIR` from
`$NUTTX_DEFCONFIG` (default
`packages/boards/nros-board-nuttx-qemu/nuttx-config/arm/defconfig`) and
rebuilds in place. Adding an SMP variant means a second configuration competing
for that tree.

**This is [issue 0743](archived/0743-nuttx-kernel-path-has-no-arch-discrimination.md)
again, in a form 0743's fix cannot see.** 0743 made `nuttx_kernel_path_for()`
read the ELF's `e_machine` so an arm consumer can never be handed the riscv
build. But `arm-uniprocessor` and `arm-SMP` are the SAME `e_machine`. A stale
SMP kernel would satisfy the arch check and silently run the uniprocessor
tests — or vice versa — with no guard anywhere. So this work needs the
build-tree question answered FIRST:

* a second NuttX checkout/tree per configuration (costs disk + a submodule-ish
  story), or
* per-config artifact staging with a config-identity stamp the resolver checks
  (the `e_machine` check widened to "which defconfig produced this?"), or
* accept serial reconfiguration and make the cost explicit in the lane.

Guessing here is how 0743 happened; pick deliberately.

Note also the shipped `qemu-armv7a/configs/smp` defconfig has **no networking**
(`CONFIG_NET=y` absent), and every nros fixture needs a transport, so the
variant is our defconfig + `CONFIG_SMP=y` + core count, not the stock config.

### ThreadX — the port exists, the board does not

`tx_thread_smp_core_exclude` type-checks against the vendored
`ports_smp/cortex_a5_smp` headers, so the arm is API-correct. But
`threadx-linux` is the Linux simulation port with the nsos-netx driver, and it
is not that port. Running this arm means bringing up a Cortex-A5 (or A9) SMP
QEMU board with NetX Duo on it — new board, new driver wiring, new fixture
family. Nothing about the existing threadx-linux lane transfers except the
application code.

### FreeRTOS — genuinely blocked, and this is the one to stop looking at

The kernel supports SMP (`configNUMBER_OF_CORES`, 201 references in
`third-party/freertos/kernel/tasks.c`, V11.2.0). The PORTS do not, for anything
we can emulate:

* `portable/ThirdParty/GCC/Posix/port.c` — **0** references to
  `configNUMBER_OF_CORES`, and its `portmacro.h` defines none of the SMP hooks
  (`portGET_CORE_ID`, `vPortYieldCore`, `vPortRecursiveLock`). The phase-370
  freertos-posix simulator board therefore cannot be made SMP by configuration;
* `portable/ThirdParty/GCC/RP2040/port.c` — 36 references, i.e. a real SMP port,
  but it targets Raspberry Pi Pico HARDWARE;
* `mps2-an385` is a single-core Cortex-M3.

So FreeRTOS core-pin acceptance needs either physical RP2040 in the loop or a
multi-core port that does not exist in this tree. Until one of those changes,
the compile gate is the whole of the coverage that is available, and that should
be stated rather than left looking like an oversight.

## Why it still matters

The compile gate closes the "a typo is invisible" hazard, which was 0260's main
worry, and issue 0655 proved that hazard was real — the Zephyr arm could never
have worked and nobody could tell, because the body was never compiled. What the
compile gate does NOT prove is that the kernel HONOURS the request: 0655 was
found by compiling, but "accepted the mask and then ignored it" is only visible
by running and asking which CPU the tier landed on. That is precisely what the
Zephyr `CorePinPlacement` cell asserts (`running_on=1`, exact line, no fallback
arm) and precisely what the other three lack.

## Acceptance

Per RTOS, either:

* a cell in `matrix::CELLS` running on a genuinely multi-core image and
  asserting PLACEMENT (`running_on=N` for N != 0, exact line, `AcceptOnly`), or
* a recorded, specific reason it cannot be, of the FreeRTOS-port kind above —
  not "needs SMP".

And for NuttX specifically: whatever the build-tree answer is, a guard that
distinguishes the configurations, so the 0743 class cannot recur one level in.
