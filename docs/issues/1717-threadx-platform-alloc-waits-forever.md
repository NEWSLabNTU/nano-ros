---
id: 1717
title: "ThreadX `nros_platform_alloc` calls `tx_byte_allocate(…, TX_WAIT_FOREVER)` — on an exhausted pool a thread BLOCKS instead of getting NULL, so every fallible-allocation path is unreachable there"
status: open
type: bug
severity: medium
area: [platform, threadx, memory]
related: [1706, 1551, 1196]
found: 2026-10-06
---

## What

`packages/platform/nros-platform-threadx/src/platform.c`:

```c
void *nros_platform_alloc(size_t size) {
    ...
    if (tx_byte_allocate(s_byte_pool, &p, (ULONG) size, TX_WAIT_FOREVER) != TX_SUCCESS) {
        return NULL;
    }
```

`TX_WAIT_FOREVER` means a thread whose request the pool cannot satisfy is
SUSPENDED until some other thread releases enough memory. If nothing does, it
waits forever, and the `return NULL` branch is reached only from a non-thread
context (where ThreadX rejects a wait option with `TX_WAIT_ERROR`).

It is the only such site. Every other `tx_byte_allocate` in the tree —
`packages/boards/nros-board-common/c/threadx_hooks.c` (×3) and
`packages/boards/nros-board-threadx-qemu-riscv64/c/board_threadx_qemu_riscv64.c`
(×3) — passes `TX_NO_WAIT`.

## Why it matters

The fallible-allocation work assumes the allocator says NO:

* `nros_rmw::fallible::try_box` / `try_box_uninit` (issues 1551 and 1706,
  PR #1723) map an allocation failure to `BAD_ALLOC` / a refused
  `declare_parameter`, so an exhausted heap degrades instead of halting.
* PR #1723 measured the clean refusal on Zephyr and the named request on
  FreeRTOS.

On ThreadX neither can happen from a thread: the 285,696-byte parameter store
(or any request larger than the pool's free space) parks the calling thread —
typically the executor's — and the image goes silent with no diagnostic. That
is an unbounded wait on the allocation path, the class issue 1196 removed from
the condvar API ("no unbounded wait is only a property of the system if it is a
property of the API").

## Not measured

Found by reading while doing issue 1706; no image was driven into this state.
PR #1723 measured threadx-linux with a byte-pool peak of 201,736 of 4,105,736
bytes, i.e. far from exhaustion, and on that board the parameter store comes
from the host allocator, not the pool. rv-virt ThreadX was not measured.

## Fix direction

`TX_NO_WAIT`, matching every other site — an allocator is not a place to wait
for another thread's `free`. Then measure on a ThreadX image driven past its
pool (rv-virt ThreadX under QEMU, or a threadx-linux image with a reduced pool)
that the allocation returns NULL and the fallible path reports, with a test
that hangs/fails before.
