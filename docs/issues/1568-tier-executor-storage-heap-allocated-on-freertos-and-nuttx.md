---
id: 1568
title: "Executor and component storage is allocated three ways and sized by guesses — the NuttX tier and C single-executor runners take 81,920 bytes for an executor that is 88,560, and FreeRTOS/NuttX tier storage is heap nothing prices"
status: open
type: tech-debt
area: boards, memory
severity: high
found: 2026-09-29
related: [issue-1551, issue-1566, issue-1115, issue-0667, issue-0245]
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
4. **NuttX snapshot.** Scope below.
