---
id: 1568
title: "Executor and component storage is allocated three ways and sized by guesses — the NuttX tier and C single-executor runners take 81,920 bytes for an executor that is 88,560, and FreeRTOS/NuttX tier storage is heap nothing prices"
status: resolved
type: tech-debt
area: boards, memory
severity: high
found: 2026-09-29
resolved: 2026-09-29
related: [issue-1551, issue-1566, issue-1115, issue-0667, issue-0245, issue-1569, issue-1570, issue-1571]
---

# Executor storage is allocated three ways and sized by guesses

The goal this issue tracks, in the words it was asked in: **every allocation
of executor (and component) storage is done ONE way, at the EXACT size the
build computed.** Issue 1551 (PR #1436) did that for Zephyr's tier runner and
nothing else. This is the survey of everything else, taken on `main` at
`da272e419`.

## Survey — where executor storage comes from, and how big it is

| Path | Storage from | Size used | Exact? |
| --- | --- | --- | --- |
| Zephyr C/C++ tiers (`zephyr_run_tiers.c`, `_in`) | entry `.bss` static | `NROS_CPP_EXECUTOR_STORAGE_SIZE` (per-build) | yes — but the runner's refusal compares against `__has_include` or a **98,304** fallback |
| FreeRTOS C/C++ tiers (`freertos_run_tiers.c`) | `nros_platform_alloc` per tier (heap_4) | per-build on the CMake lane (measured `0x159a0` = 88,480 in the realtime-c object); **81,920** fallback on the cargo lane | CMake lane yes; invisible to `mem-report` and to the link either way |
| NuttX C/C++ tiers (`nuttx_run_tiers.c`) | `nros_platform_alloc` per tier | **81,920 — always.** The seam is compiled by `build.rs` (`compile_entry_seams`) with NuttX include roots only, so `__has_include(<nros/nros_cpp_config_generated.h>)` is false and the fallback is what ships | **no — 6,640 bytes SHORT** |
| C single-executor, every RTOS (`nros_rtos_run_components.c`) | `nros_platform_alloc` | per-build where the header is visible; **98,304** (Zephyr) / **81,920** (else) fallback; on NuttX it rides the same header-less seam archive | NuttX: **short**, as above |
| C++ single-executor, every board (`Node::GlobalStorageHolder<0>::storage`) | COMDAT `.bss` | `NROS_CPP_EXECUTOR_STORAGE_SIZE` | yes, except NuttX (below) |
| Rust single executor (`nros_node::executor::backing::EXECUTOR_BACKING`) | named `.bss` | type-derived | yes (phase-392 W6) |
| Native/Linux (`nros_board_native_run_tiers_ns`) | boot `MaybeUninit<CppContext>` on the stack; tier contexts `Box`ed | `size_of` | yes — typed Rust, host process |

### The NuttX shortfall, measured

`libnros_nuttx_run_tiers.a` from the realtime-c arm fixture
(`examples/workspaces/realtime-c/build/nuttx-zenoh-nuttx-qemu-arm/…/out/`):

```
<nuttx_spawn_next_tier>:        20: mov r0, #81920   @ 0x14000  ; -> nros_platform_alloc
<nros_board_nuttx_run_tiers>:   40: mov r0, #81920   @ 0x14000  ; -> nros_platform_alloc
```

and the per-build header the SAME build wrote beside it
(`cargo-target/nros-cpp-generated/nros/nros_cpp_config_generated.h`):

```
NROS_CPP_EXECUTOR_STORAGE_SIZE 88560   (arm)      88480 (riscv32)
```

`nros_cpp_init` / `nros_cpp_executor_open_over_session` write a `CppContext`
into that block with no size argument. So every NuttX C/C++ tiered image hands
each tier's executor a heap block **6,640 bytes smaller than the object built
in it** — issue #245 exactly (81,920 was chosen then as "80 KiB of headroom"
over a 79,304-byte executor; the executor has since grown past it). It does
not fail visibly because NuttX's heap is 126 MiB (arm) / 32 MiB (riscv): the
overrun lands in whatever the allocator put next, and the carved backing the
tail holds is touched only as entities register.

### The hard-coded component literals

`packages/api/nros-c/include/nros/component.h` falls back to literals when the
per-build header is absent: publisher **560**, action server **128**, action
client **64**, and service client **4632** with NO per-build arm at all.
Issue 1566 measured the publisher literal 80 bytes short (560 vs 640) and it
corrupted memory; it switched the publisher to `NROS_PUBLISHER_SIZE +
sizeof(void*)` and recorded the service client as unchecked. Per-build values
from the same fixtures: publisher 548, service client **520**, action server
72 (arm) / 84 (riscv), action client 24. The service-client buffer is sized
from a literal nine times the object on every build — safe, but not the build's
number, and nothing would say so if the object outgrew it.

### The NuttX committed snapshot

On NuttX `<nros/nros_cpp_config_generated.h>` is the committed
`nros_cpp_config_generated_nuttx.h` (issue 1115): `NROS_CPP_EXECUTOR_STORAGE_SIZE
98312`. That is an UPPER bound over the measured 88,560 / 88,480 — 9,752 /
9,832 bytes over per executor — so what the C++ `GlobalStorageHolder` and any
entry static get on NuttX is the snapshot's number, not the build's. It cannot
be exact by construction: one file stands for two architectures that measure
differently.

### Invisible and unpriced

On FreeRTOS each tier also takes its task stack from the same heap_4
(`xTaskCreate`, 256 KiB default per tier). Neither the executor blocks nor the
stacks appear in `mem-report` (it reads symbols) or at link; the heap is sized
by `NROS_FREERTOS_HEAP_KB` (3,072 KiB default on mps2-an385, 32,768 on
mps3-an536), and nothing multiplies the tier count against it.

## Fix direction

Mirror #1436, do not invent a second mechanism:

1. **One size, from the library that writes it.** A runner cannot know the
   executor size at compile time on every lane (the NuttX seam and the FreeRTOS
   cargo glue see no nros header at all). The linked `nros-cpp` does. Export it
   (`nros_cpp_executor_storage_size()`) and a single refusal
   (`nros_cpp_executor_storage_check()`), and delete every fallback constant
   from the runners.
2. **One mechanism.** `run_tiers_takes_storage` on for FreeRTOS and NuttX
   (`nros_board_{freertos,nuttx}_run_tiers_in`); the C single-executor runner
   gets the same `(storage, bytes)` twin and its flag. The `_ns` spellings stay
   for entries generated before this, now sized exactly.
3. **Exact literals.** `component.h` takes the per-build size or refuses to
   compile, never a guess.
4. **NuttX snapshot.** Measured, raised where it had fallen below the build, and the exact fix scoped as issue 1569.

## Resolution

### One way

| Path | After |
| --- | --- |
| Zephyr / FreeRTOS / NuttX C and C++ tiers | entry `.bss` `__nros_tier_executor_storage[n][…]` → `nros_board_<rtos>_run_tiers_in(…, storage, stride)` |
| C single executor, every RTOS (ThreadX included) | entry `.bss` `__nros_executor_storage[…]` → `nros_board_rtos_run_components_in(…, setup, storage, bytes)` |
| C++ single executor | unchanged — already `.bss` (`GlobalStorageHolder<0>`); `nros::init` now takes the same refusal |
| Rust single executor | unchanged — `.bss` `EXECUTOR_BACKING` since phase-392 W6 |
| Rust tiers (non-boot) | **not unified** — `open_with_session_handle` falls through to `Box::leak` on the heap at `ExecutorSizing::DEFAULT` (exact by type, invisible to `mem-report`): issue 1571 |
| Native / Linux | unchanged — Rust runners, `MaybeUninit<CppContext>` on the stack; host process |

- ONE flag: `CAbiRunners::takes_executor_storage` (was Zephyr-only
  `run_tiers_takes_storage`), true for every RTOS family, read by both packs;
  `nros-entry-lower` test `every_rtos_runner_takes_its_executor_storage_from_the_entry`
  refuses a family added with a heap runner.
- ONE size and ONE refusal, from the linked library:
  `nros_cpp_executor_storage_size()` / `nros_cpp_executor_storage_check()`.
  Every runner fallback (81,920 / 98,304) is deleted; the runner TUs no longer
  probe for a header at all. The `_ns` spellings stay for older entry TUs and
  take one heap block at the library's size.

### Exact sizes

- `component.h`: publisher / action server / action client / service client
  take the per-build macro each `nros_cpp_*_create` writes; without the
  header they expand to an undeclared identifier naming the missing header
  (a refusal at the use site, never a guess). No consumer needed a fallback:
  the only header-less compile is one that never links nros-cpp.
- NuttX snapshot: **did not match** — at this commit the arm build measures
  seven sizes ABOVE it (publisher 596 vs 560, subscriber 620, service server
  568, …), a live 36-byte publisher overrun on realtime-c. Raised; exact
  sizing on NuttX is issue 1569, and the rebuild edge that hid a header change
  from the NuttX image is issue 1570.

### Measured (acceptance)

FreeRTOS mps2-an385, realtime-c (2 tiers), same build dir, the "before" being
the `_ns` heap path an older entry takes:

| | before (`_ns`, heap) | after (`_in`, `.bss`) |
| --- | ---: | ---: |
| `mem-report` `__nros_tier_executor_storage` | absent | 178,144 (2 × 89,072) |
| RAM `.bss + .data` | 3,538,968 | 3,717,112 (+178,144) |
| boot `heap peak` (heap_4) | 544,176 | 365,968 (−178,208) |

A MOVE, not a saving: the bytes left heap_4 and became a linker-visible
symbol. The same image with the static widened to 8 tiers FAILS AT LINK
(`region 'RAM' overflowed by 65440 bytes`); the same image passing a stride
8 bytes short boots to `nros: freertos run_tiers: executor storage refused …`
and opens no session. Five unit tests pin the refusal (short, non-multiple of
8, misaligned, null, exact).

NuttX qemu-armv7a, realtime-c: `__nros_tier_executor_storage` 196,624 (2 ×
98,312 — the snapshot's number; exact would be 178,144, issue 1569), component
instance 0x238 → 0x288 after the snapshot raise. `realtime_tiers` e2e:
`freertos/c` and `nuttx-arm/c` ran and passed (tiers dispatch; NuttX tiers
deliver to host sinks).

### Not done

- FreeRTOS tier TASK STACKS still come from heap_4 (`xTaskCreate`, 256 KiB
  default per tier). Moving them needs `configSUPPORT_STATIC_ALLOCATION 1`
  (mps2-an385 has 0), the idle/timer `vApplicationGet*TaskMemory` hooks, and
  the entry emitting `StackType_t[n][words]` + `StaticTask_t[n]` beside the
  executor static — the stack size is a per-tier spec value the emitter
  already has, but the 256 KiB default lives in the runner and would move to
  the emitter. Not changed here.
- riscv32 NuttX, C++ and Zephyr tiered images were not rebuilt on this branch.
- The Rust tier road (issue 1571), NuttX exactness (issue 1569) and the NuttX
  rebuild edge (issue 1570) are filed separately.
