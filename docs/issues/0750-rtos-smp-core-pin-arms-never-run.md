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

### NuttX — closest, and the plan is B + C below

**Correction (2026-08-21), same day this issue was filed.** The first draft said
the shared kernel tree is the obstacle and listed "second tree / config-identity
stamp / accept serial reconfiguration" as three open options. That framing was
wrong about the hazard, because it missed a layer that already exists.

`scripts/nuttx/build-nuttx.sh` does reconfigure `$NUTTX_DIR` in place from
`$NUTTX_DEFCONFIG`. But **consumers do not link the tree.** phase-339 W2 gives
each build a per-arch SNAPSHOT, `nros-nuttx-export-<arch>/`, carrying its own
`.nros-export-key` — and that key is already `HEAD:sha256(defconfig)`, i.e. it
ALREADY distinguishes two configs of the same arch. The script's own comment
says why: "records what produced it, so freshness never depends on the shared
tree."

So the shared tree is a BUILD-time serialization point, not a consumption
hazard. The only thing missing is granularity: the snapshot DIRECTORY is keyed
on `<arch>`, and `arm` and `arm-smp` are both `arm`, so the two configs would
land in one directory and overwrite each other — while the key file inside
correctly reports a mismatch and forces a rebuild. That is a thrash, loud and
slow, not the silent wrong-kernel the first draft feared.

It is still [issue 0743](archived/0743-nuttx-kernel-path-has-no-arch-discrimination.md)
one level in — `nuttx_kernel_path_for()` reads `e_machine`, and
arm-uniprocessor and arm-SMP share it — so the RESOLVER still cannot tell the
two apart. Both halves want the same fix.

**B — widen the identity from arch to config.** The snapshot dir becomes
`nros-nuttx-export-arm-smp/` beside `-arm`; the id comes from the defconfig's
own directory name (`nuttx-config/<id>/defconfig`), so it is derived, not a
hand-maintained list. `nuttx_include_root()`'s `snapshot_arch` argument becomes
that id — its callers already pass an arch string, so the change is the value,
not the shape. The resolver then asks for a NAMED CONFIG and can check the
snapshot's key/`config.h` (does it actually have `CONFIG_SMP`?) instead of
inferring from `e_machine`, which is the check that cannot work here.

**C — one more lane, shaped exactly like riscv.** `build-fixtures-arm-smp`
gated on `nros_lane_wants_platform nuttx-arm-smp`, mirroring
`build-fixtures-riscv` line for line, so a lane naming no SMP coordinate pays
nothing. `NUTTX_DEFCONFIG` points at the new `nuttx-config/arm-smp/defconfig`.
Then a `[[fixture]]` row and a `CorePinPlacement`-style cell asserting the exact
`running_on=1` line, `AcceptOnly`, the way the Zephyr a53 cell does.

**A — a second NuttX tree — is NOT recommended.** `NUTTX_DIR` is already
env-overridable so the wiring is trivial, and it would buy parallel builds. But
NuttX is a SUBMODULE: a second checkout means either a second submodule entry
(two pins for one upstream, which must move forward together under the
forward-only rule, with `check-submodule-pins` widened to both) or a worktree
the pin gate does not model. That is a permanent drift liability bought to avoid
a serialization cost that C only pays when a lane actually names SMP. It also
buys nothing the snapshot layer does not already provide, now that the snapshot
is understood.

Note also the shipped `qemu-armv7a/configs/smp` defconfig has **no networking**
(`CONFIG_NET=y` absent), and every nros fixture needs a transport, so the
variant is our defconfig + `CONFIG_SMP=y` + core count, not the stock config.
That is the real work here; B and C are small beside it.

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

And for NuttX specifically, B before C — the snapshot identity widened from
`<arch>` to `<config>` BEFORE a second arm config exists, so the two never share
a directory and the resolver can name what it wants. Building the fixture first
and adding the guard after is the ordering that produced 0743.
