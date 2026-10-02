---
id: 1598
title: "FreeRTOS tier task stacks still come from heap_4 via `xTaskCreate` — 256 KiB per tier, priced by nothing at link time"
status: resolved
resolved: 2026-10-02
type: tech-debt
area: [memory, freertos]
severity: medium
found: 2026-10-01
related: [issue-1568, issue-1571, issue-1146, issue-0667, issue-1232]
---

## What is left

After issues 1568 and 1571, a FreeRTOS tier's EXECUTOR storage is a `.bss`
static the entry owns. Its TASK STACK is not: each tier task is created with
`xTaskCreate`, which takes the stack from heap_4. The default is 256 KiB per
tier, from issue 1146's measurement, charged per task. Issue 1568 recorded
this and deliberately left it.

So the largest per-tier cost on FreeRTOS is still invisible to the linker and
to `mem-report`. An image that cannot hold its tiers' stacks fails at boot as
`*** MALLOC FAILED ***` (CLAUDE.md: an undersized stack lands as a malloc
failure, not a stack overflow), not at link.

## What moving them needs (from issue 1568's assessment)

- `configSUPPORT_STATIC_ALLOCATION 1` in each FreeRTOS board config
  (mps2-an385 currently has 0), plus the `vApplicationGetIdleTaskMemory` /
  `vApplicationGetTimerTaskMemory` hooks it then requires.
- The generated entry emitting `StackType_t __nros_tier_stacks[n][words]`
  plus `StaticTask_t[n]`, sized from each tier's `stack_bytes` (the floor
  rule of issue 0667 still applies: the port may raise it).
- The 256 KiB default moving from the runner to the emitter, so the number is
  stated where it is reserved.
- `xTaskCreateStatic` in the C and Rust FreeRTOS tier runners. The Rust
  `nros::main!` road needs the same static.

## Acceptance

- `mem-report` names the tier stacks on a FreeRTOS tiered image.
- The boot heap peak drops by the moved amount (a move, not a saving).
- An image whose stacks exceed RAM fails at link.
- Every FreeRTOS tiered fixture still boots and both tiers tick.

## Resolution

Resolved on `fix/1597-1598-1232-tier-storage-and-stacks` (commits `f448768b6`
C/C++, `6816ffd9f` Rust).

- `configSUPPORT_STATIC_ALLOCATION 1` in the family `FreeRTOSConfig.h` (shared
  by mps2-an385 / mps3-an536 / s32z270; the POSIX board already had it), and
  `vApplicationGet{Idle,Timer}TaskMemory` in `c/freertos_hooks.c`. As a side
  effect the idle and timer stacks moved from heap_4 to `.bss` too (1,024 +
  2,048 B + two 104 B TCBs).
- `nros_tier_task_memory_t {stack, stack_bytes, tcb}` in `<nros/main.h>`
  (canonical; mirrored in the runner, `check-ffi-struct-mirrors` family 3).
- `nros_board_freertos_run_tiers_tasks_in(…, storage, stride, task_memory)`:
  each spawned tier is `xTaskCreateStatic`d over the entry's row; the boot
  tier's row must be empty and every spawned row present and at least
  `configMINIMAL_STACK_SIZE`, else refused before the session opens. `_in`
  delegates with NULL (`xTaskCreate`, unchanged for an older entry).
- The generated C/C++ entry includes `<nros/tier_task_memory_freertos.h>` and
  declares `__nros_tier_stack_<i>` / `__nros_tier_tcb_<i>` per spawned tier
  (`NROS_TIER_TASK_MEMORY_DEFINE`, which raises a declaration below the port's
  floor — issue 0667). The 256 KiB default moved from the runner to
  `nros-entry-lower` (`CAbiRunners::tier_task_memory`), where it is reserved.
- Rust `nros::main!`: one `nros::TierTaskMemory<words>` static per spawned tier
  (stack exact; TCB region a stated 512 B bound, `TIER_TASK_TCB_U64S`, checked
  against `sizeof(StaticTask_t)` by `nros_freertos_create_task_static`) and
  `Mps2An385::run_tiers_with_task_memory`. Same `TierTaskMemory::stacks`
  lowering as the C/C++ packs, so the Rust road's undeclared tier stack is now
  the family's 256 KiB, not `app_stack_bytes` (128 KiB) — see below.

### Measured (mps2-an385, release)

realtime-c `demo_bringup:freertos` (2 tiers), same build dir; "before" is the
same image with its entry pointed back at `_in` (heap stacks):

| | before (`_in`) | after (`_tasks_in`) |
| --- | ---: | ---: |
| `nm`: `__nros_tier_stack_0` / `__nros_tier_tcb_0` | absent | 262,144 / 104 |
| `size` bss | 3,723,928 | 3,986,184 (+262,256) |

- **Fails at LINK when it does not fit:** the same entry with the tier
  declaring 16 MiB → `region 'RAM' overflowed by 16312840 bytes`.
- realtime-rust `demo_bringup:freertos_realtime`: `nm` names
  `__NROS_TIER_TASK_MEMORY_0` 262,656 B (stack + 512 B TCB bound) beside
  `__NROS_TIER_EXECUTOR_BACKING`; boots under QEMU against `rmw_zenohd` with
  BOTH tiers dispatching (`on_ctrl: first publish OK (tier high …)`,
  `on_telem: first publish OK (tier low …)`), `high stack peak 29416 of
  262144`, `heap peak 169592`.

A MOVE, not a saving: the bytes leave heap_4 and become linker-visible.

### Not measured

- **The boot heap-peak drop.** The C image under QEMU here reached `Network
  ready` and the app-task peaks but never the tier spawn — identically for the
  before and after builds (heap peak 92,032 both), so it is a property of this
  environment/image, not of the change, and it gives no before/after for the
  heap. No "before" Rust image was built (it would need a rebuild of the parent
  commit). By construction the drop is one `stack_words * 4` heap_4 block + a
  TCB per spawned tier.
- FreeRTOS `realtime_tiers` e2e cells were not run (fixture stamps not built,
  see issue 1597's note); the C++ FreeRTOS image and mps3-an536 / s32z270 were
  not built. The freertos-posix board (also FreeRTOS family) was not built.
- **Behaviour change on the Rust road, deliberate but unmeasured beyond one
  image:** an undeclared tier's stack is now 256 KiB (the C road's, issue #126)
  instead of `app_stack_bytes`. The measured peak above (29 KiB) says both are
  generous; settling ONE measured default for both roads is left to a
  follow-up rather than guessed here.

## Follow-up (2026-10-02): the heap default still budgeted for the moved stacks

The C/C++ road compiles the kernel in cmake (`freertos_kernel`), so its
`ucHeap` is `FreeRTOSConfig.h`'s 3 MiB default whatever the RMW. Once the
stacks moved to `.bss` that default still made room for them, so the bytes were
reserved twice. The unbuilt C++ image above is the one that showed it: the
nightly `freertos` lane failed on realtime-cpp `demo_bringup:freertos` (three
tiers, two spawned) with `region 'RAM' overflowed by 151024 bytes`. Measured
locally: `ucHeap` 0x300000 beside two 0x40000 `__nros_tier_stack_*`.

Fix: the entry sidecar states the spawned tiers' total
(`NROS_TIER_STACKS_BSS_BYTES`, from the same lowering that renders the TU),
`nano_ros_entry` defines `NROS_FREERTOS_TIER_STACKS_IN_BSS_KB` PUBLIC on
`freertos_kernel`, and the header's DEFAULT subtracts it. An explicit
`NROS_FREERTOS_HEAP_KB` is not touched. After: `ucHeap` 0x280000, the image
links, and every target that compiles `FreeRTOSConfig.h` and reads
`configTOTAL_HEAP_SIZE` carries the same define (issue 1197).

Still open: the cargo road's derivation
(`nros_board_common::freertos_config::default_heap_bytes`) charges
`app_stack_bytes` for three task slots, two of which are tiers whose stacks are
now in `.bss` too. It does not overflow anything (704 KiB), so it is left for
whoever next re-measures that heap, rather than changed here without a run.
