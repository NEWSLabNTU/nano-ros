---
id: 880
title: "192 KiB of tightly-coupled memory sits at 0 % while SRAM is exhausted —
  the Zephyr images place nothing in ITCM or DTCM"
status: open
type: bug
area: platform, embedded
related: [issue-0852, issue-0810]
---

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
