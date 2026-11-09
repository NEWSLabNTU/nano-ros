---
id: 880
title: "192 KiB of tightly-coupled memory sits at 0 % while SRAM is exhausted —
  the Zephyr images place nothing in ITCM or DTCM"
status: resolved
resolved: 2026-11-09
resolved_in: "phase-512 (measured on the island's board image, island 6b09fa7, nano-ros ac23c0155b)"
type: bug
area: platform, embedded
related: [issue-0852, issue-0810, phase-474, phase-394, phase-512]
---

**Home: [phase-512](../../roadmap/phase-512-on-hardware-measurements.md)**. Re-homed 2026-11-05 from phase-474 (archived): the TCM placement is measured on the island's S32K344.

## Resolved 2026-11-09: the TCM is in use, and SRAM has 63 KiB spare

The island's MR-CANHUBK344 image (`just board-build`, island main 6b09fa7,
nano-ros ac23c0155b, serial zenoh link), clean build, the linker's table:

```
FLASH   690 188 / 4 144 896   16.65 %
RAM     262 976 /   327 680   80.25 %
ITCM     18 108 /    65 536   27.63 %
DTCM    117 984 /   131 072   90.01 %   (the link asserts an 8 KiB guard)
```

The gap this issue was filed for is gone. DTCM is the fuller of the two
data regions, and ITCM holds relocated code. What put them there:

- `CONFIG_CODE_DATA_RELOCATION=y` with `CONFIG_NROS_ZENOH_SERIAL_TCM=y`: the
  serial link's code in ITCM (`.itcm_text_reloc`, 18,108 B) and its buffers
  in DTCM (`_z_serial_rx_storage` and `_z_serial_tx_storage`, 4,096 B each,
  static: the receive loop no longer allocates per frame, item 5 below).
- The executor's level stacks, `__nros_levels_backing`, 33,128 B in DTCM
  (phase-509), plus the per-level executor storage and component buffers
  (`__nros_sched_task_executor_storage` 2,816 B, `__nros_comp_buf_0..2`
  10,216 B).
- The island's RAM trace buffer, `ram_tracing`, 53,248 B in DTCM.
- The eDMA descriptor pool (`dma_tcdpool0`, 2,048 B,
  `CONFIG_DMA_MCUX_USE_DTCM_FOR_DMA_DESCRIPTORS`).

Top SRAM consumers now, from the ELF:

| bytes | symbol | kind |
| ---: | --- | --- |
| 95 952 | `nros_platform::zephyr_heap::HEAP` | the one heap (`CONFIG_NROS_ZEPHYR_HEAP_SIZE` 94,208) |
| 24 640 | `nros_rmw_zenoh::shim::subscriber::SMALL_PAYLOADS` | static pool |
| 20 480 | `nros_thread_stacks` | task stacks |
| 19 240 | `g_sessions` | zenoh session |
| 16 384 | `z_main_stack` | main |
| 16 384 | `__nros_sched_task_stack_1` | sched task stack |
| 16 384 | `__nros_sched_task_stack_2` | sched task stack |
| 7 168 | `nros_rmw_cffi::rust_adapter::static_subscriber_storage::SLOTS` | |
| 6 144 | island supervisor stack | |
| 4 096 | `nros_rmw_cffi::MESSAGE_INFO_TABLE` | |

The secondary findings:

- **Three heaps: now one.** `CONFIG_HEAP_MEM_POOL_SIZE=0`, and
  `kheap__system_heap` is 572 B (the POSIX layer's two 256 B pool additions). The nano-ros heap is
  the only heap of size.
- **`malloc_arena`: gone.** `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=0`, and the
  ELF has no `malloc_arena` symbol.

The remaining opportunities below, as they stand:

1. `LARGE_PAYLOADS` no longer exists; the zenoh shim's payload pool is
   `SMALL_PAYLOADS`, 24,640 B.
2. Heap unification: done, as above.
3. `z_main_stack` stays in SRAM. DTCM has 13,088 B left, 8,192 B of which
   is the link's guard, so there is no room for it there, and SRAM is not
   short.
4. ITCM: used, by the serial link's code.
5. The RX path's per-frame allocations: replaced by the static storage above.

No residual gap is filed. SRAM has 64,704 B spare, and the two 16 KiB sched
task stacks could follow `CONFIG_NROS_ZEPHYR_SCHED_TASK_STACKS_IN_DTCM` only
if the image gave up DTCM elsewhere (the trace buffer is the large tenant).
That is a choice for the image, not a nano-ros gap.

## The original filing

## Measurement

The MR-CANHUBK344 action image, clean build, before this change:

```
RAM   323 528 / 327 680   98.73 %
ITCM        0 /  65 536    0.00 %   @ 0x00000000
DTCM        0 / 131 072    0.00 %   @ 0x20000000
```

The image could not be instrumented — adding a stack sentinel and a debug
console pushed it past the ceiling and the node stopped completing entity
creation — while **192 KiB of memory on the same die went unused.**

Both regions are already declared. The devicetree gives them
`zephyr,memory-region = "ITCM"` / `"DTCM"`, and Zephyr's generated linker
script emits matching output sections:

```
ITCM (NOLOAD) : { KEEP(*(ITCM)) KEEP(*(ITCM.*)) } > ITCM
DTCM (NOLOAD) : { KEEP(*(DTCM)) KEEP(*(DTCM.*)) } > DTCM
```

Nothing in nano-ros or in the images ever placed a symbol in either. Grepping
the tree for a section attribute finds exactly one, and it is `.noinit`.

## Where the SRAM goes

Top consumers of the 323 KiB, from the ELF:

| bytes | symbol | kind |
| ---: | --- | --- |
| 67 248 | `nros_platform::zephyr_heap::HEAP` | nano-ros heap |
| 65 536 | `nros_rmw_zenoh…subscriber::LARGE_PAYLOADS` | static pool |
| 49 152 | `nros_thread_stacks` | 6 x 8 KiB task stacks |
| 32 852 | `kheap__system_heap` | Zephyr heap |
| 26 568 | `nros_rmw_zenoh…service::SERVICE_BUFFERS` | static pool |
| 16 384 | `z_main_stack` | executor thread |
| 16 384 | `…subscriber::SMALL_PAYLOADS` | static pool |
| 8 192 | `…static_subscriber_storage::SLOTS` | |
| 8 192 | `malloc_arena` | **dead** — see below |

Two things stand out beyond the TCM gap.

**Three heaps, 108 KiB.** `HEAP` (67 KiB) + `kheap__system_heap` (33 KiB) +
`malloc_arena` (8 KiB). This is the case the heap-unification campaign exists to
make.

**`malloc_arena` is dead weight, again.** The image contains no `malloc` and no
`free` — nano-ros routes every allocation through `nros_platform_alloc` — yet
the arena is reserved because `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` reserves
its pool whether or not any caller survives the linker. Dead code is collected;
dead *reservations* are not. This is the second time the same 8 KiB has been
found on this board.

## What landed here

`CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM` places the task stack array in DTCM using
`Z_KERNEL_STACK_ARRAY_DEFINE_IN`. Off by default: a DTCM is a property of the
part, not of Zephyr, and a SoC without the region should fail to link rather
than silently place elsewhere.

Stacks are the right first tenant. They are CPU-private, never the target of a
DMA transfer, and TCM access does not contend with the system bus — so this is
a latency improvement as well as an SRAM saving.

Measured on the action image, with the libc arena also zeroed:

| | SRAM | DTCM |
| --- | ---: | ---: |
| before | 98.73 % | 0 % |
| after | **85.60 %** | 37.50 % |

and the board boots, registers every entity and reaches its ready state with
zero faults — which the 98.73 % image could not do.

## Remaining opportunities, in order of size

1. **`LARGE_PAYLOADS`, 64 KiB** → DTCM. 80 KiB of DTCM is still free. **Verify
   first** whether this pool is ever a DMA target: on Cortex-M7 the TCMs hang
   off the CPU's private bus and are typically unreachable from other bus
   masters, so moving a buffer there would break an eDMA path. The serial link
   is polled/ISR today, but issue 0852's fix direction includes DMA.
2. **Heap unification, up to ~40 KiB.** Three heaps sized independently for
   worst cases that do not co-occur.
3. **`z_main_stack`, 16 KiB.** Zephyr owns the placement; needs a different
   mechanism than the array above.
4. **ITCM, 64 KiB, still entirely unused.** It does not relieve SRAM — code
   lives in flash, which is at 8 % — but `CONFIG_CODE_DATA_RELOCATION` would put
   hot paths in zero-wait-state memory. A determinism lever, not a capacity one.
5. **The RX path allocates twice per frame** (`_Z_SERIAL_MAX_COBS_BUF_SIZE` +
   `_Z_SERIAL_MFS_SIZE`) inside the receive loop. Static buffers would cut both
   peak heap and an unbounded-latency call on the hot path.
