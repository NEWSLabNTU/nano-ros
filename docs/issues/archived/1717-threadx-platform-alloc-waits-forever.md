---
id: 1717
title: "ThreadX `nros_platform_alloc` calls `tx_byte_allocate(…, TX_WAIT_FOREVER)` — on an exhausted pool a thread BLOCKS instead of getting NULL, so every fallible-allocation path is unreachable there"
status: resolved
type: bug
severity: medium
area: [platform, threadx, memory]
related: [1706, 1551, 1196, 1719, 1720]
resolved_in: "branch issue-1717-threadx-alloc-nowait"
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

## Resolution

`nros_platform_alloc` passes `TX_NO_WAIT`. `nros_platform_realloc` goes
through it, so it inherits the fix.

**Measured on a real ThreadX kernel driven past its pool** — the ThreadX linux
port smoke (`tests/threadx-c-smoke`, `just threadx_linux test-c-port`), which
links this port's `platform.c` over a 256 KiB `TX_BYTE_POOL` and now exhausts
it in 16 KiB `nros_platform_alloc` chunks from a ThreadX thread, then releases
them and allocates again:

| | result |
| --- | --- |
| before (`TX_WAIT_FOREVER`) | prints `exhausting the pool…`, then nothing; `timeout 15` kills it, **rc 124** after 15.0 s |
| after (`TX_NO_WAIT`) | `alloc returned NULL after 13 chunk(s)`, `pool usable again after release`, **PASS, rc 0** in 0.25 s |

The chunks are deliberately SMALLER than the pool: ThreadX's error-checking
layer (`txe_byte_allocate.c`) refuses a request larger than the WHOLE pool with
`TX_SIZE_ERROR` before any wait, so the hang needed a request the pool could
satisfy if someone freed — i.e. a pool that is busy, not one that is small.
That narrows the issue's description in one respect: on rv-virt (a 4 MiB pool)
the 285,696-byte parameter store would hang only once the pool's FREE space
fell below it, never because the pool is too small outright.

**What was not measured**: an rv-virt ThreadX Rust image with
`param-services` driven past its pool under QEMU, i.e. the parameter store's
`parameter store refused: needs N bytes` line printed on ThreadX. No in-tree
rv-virt Rust example declares parameters, and the pool is not independently
sizable (only `NROS_EXECUTOR_BACKING_U64S` shrinks it). Everything above
`nros_platform_alloc` on that path — `try_box_uninit` → `None` →
`report_parameter_store_refused` — is platform-independent Rust, unit-tested
by `RefuseParameterStore` and measured on Zephyr and FreeRTOS in PR #1723; the
ThreadX-specific link was the NULL return, and that is what the smoke proves.

**Sweep** — every allocator entry point on every port, plus every call of a
kernel allocation primitive that takes a wait option:

* `nros_platform_alloc`/`realloc` — FreeRTOS (`pvPortMalloc`), Zephyr (rlsf
  arena under a spinlock), POSIX (`malloc`): none takes a wait option, none can
  block on exhaustion. ThreadX was the only one.
* `z_malloc`/`z_realloc` (`zpico-sys/c/zpico/platform_aliases.c`) forward to
  the above. No XRCE allocator exists; Cyclone's `ddsrt_malloc` routes to the
  platform heap.
* Wait-taking primitives: 11 calls in tracked code (`tx_byte_allocate` ×9 incl.
  the timer pool and both boards, `nx_packet_allocate` ×1 in
  `virtio-net-netx`, and the smoke's own) — all `TX_NO_WAIT`/`NX_NO_WAIT` after
  this fix. No Zephyr `k_heap_alloc`/`k_mem_slab_alloc` call exists.

**Gate** — `just check allocator-never-waits`
(`scripts/check-allocator-never-waits.py`, fast line): every call of
`tx_byte_allocate`, `tx_block_allocate`, `nx_packet_allocate`,
`k_heap_{alloc,aligned_alloc,calloc,realloc}` or `k_mem_slab_alloc` passes
that kernel's no-wait token, read from the wait ARGUMENT (so a numeric or
`K_MSEC` timeout fails too, and a file that legitimately waits forever on a
mutex does not). Self-test plants six violations (forever, numeric, wrapped,
Zephyr `K_FOREVER`, `K_MSEC`, truncated) beside comment/declaration/OK cases;
negative control: restoring `TX_WAIT_FOREVER` in `platform.c` fails it naming
`platform.c:110`. It also refuses a vacuous pass (zero calls found).

Found alongside: `scripts/check-no-unbounded-condvar-wait.sh` (issue 1196, the
same "no unbounded wait" rule for the condvar) had NO recipe, so no lane ran it;
it is now `just check no-unbounded-condvar-wait` and passes. Filed: issue 1719
(ThreadX/FreeRTOS realloc copies the new size out of the old block) and issue
1720 (the C-port smokes, including this fix's runtime proof, run in no lane).
