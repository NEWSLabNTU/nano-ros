---
id: 1719
title: "ThreadX and FreeRTOS `nros_platform_realloc` copy the NEW size out of the OLD block — a grow reads past the end of the allocation"
status: resolved
type: bug
severity: low
area: [platform, threadx, freertos, memory]
related: [1717]
resolved_in: "branch issue-1719-realloc"
found: 2026-10-06
---

## What

Both ports implement realloc as alloc + copy + free, and copy `size` bytes —
the REQUESTED size — from the old pointer:

* `packages/platform/nros-platform-threadx/src/platform.c` —
  `memcpy(out, ptr, size);`
* `packages/platform/nros-platform-freertos/src/platform.c` —
  `memcpy(out, ptr, size);` ("best-effort copy … the caller is expected to
  track that out-of-band")

On a GROW, `size` exceeds the old block, so the copy reads past its end: into
the pool's next block header and whatever follows. On a part with an MPU
region boundary there that is a fault; otherwise it copies garbage into the
tail, which the caller was about to overwrite anyway — so it is latent rather
than visibly wrong. Zephyr's port delegates to the rlsf arena's own realloc
and is not affected; POSIX calls libc `realloc`.

## Who reaches it

`z_realloc` in `packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c`
forwards to it. The pinned zenoh-pico's core `src/` declares `z_realloc` but
no code path calls it (only the per-platform definitions), so no measured
in-tree caller exists today. Found by reading while doing issue 1717.

## Fix direction

The old size IS recoverable on ThreadX: a byte-pool block is preceded by the
pool's own header, whose next-block pointer bounds the block (that is how
`_tx_byte_release` merges). Copy `min(old, new)`. FreeRTOS heap_4 keeps
`xBlockSize` in the `BlockLink_t` immediately before the returned pointer
(`vPortGetHeapStats` exists; there is no public per-block size query, but
heap_4's layout is fixed by the kernel version the board pins). Either way the
copy length must come from the allocator, never from the request.

## Resolution

The copy length now comes from the allocator that owns the block, in every
port that emulates realloc:

* **ThreadX** — the byte pool's own block header, no new header:
  `[UCHAR *next_block][ALIGN_TYPE owner]` sits immediately before the returned
  pointer (offsets spelled as `_tx_byte_release` spells them), and
  `next_block - ptr` is the block's exact span. A header whose owner slot is
  not the registered pool is refused with NULL.
* **FreeRTOS heap_4 / heap_5** (every in-tree board) — heap_4's
  `BlockLink_t { next; xBlockSize }`, padded to `xHeapStructSize`, mirrored
  from the pinned V11.2.0 kernel: usable = `(xBlockSize & ~allocated_bit) -
  xHeapStructSize`. A header without the allocated bit is refused with NULL.
* **FreeRTOS heap_3** (the freertos-posix board) and **ESP-IDF** — the C
  library's `realloc`, which owns the same blocks (`pvPortMalloc` is `malloc`
  / `heap_caps_malloc` there). The ESP-IDF arm is unmeasured: no IDF build
  exists in-tree (RFC-0065 D3).

**Sweep** — `git grep -n 'nros_platform_realloc\|fn realloc' -- ':!*.md'`:
POSIX, NuttX (POSIX source) and `examples/native/c/custom-platform` call libc
`realloc`; Zephyr calls rlsf's `reallocate`; the bare-metal boards
(mps2-an385, stm32f4, esp32-qemu) reach `zpico_alloc::FreeListHeap::realloc`.
That last one had the same class in its #190 FOREIGN-pointer branch — it
copied the requested `size` out of a block it had no header for, "its true
length is unknown but ≥ what the caller is reallocating around", which is
backwards on a grow. It now refuses (NULL, the foreign block untouched,
counted in `foreign_free_count`). Its slab and rlsf branches were already
bounded.

**Tests, each failing before and passing after:**

* `tests/c-port-smoke-common/realloc_probe.h`, run by the ThreadX linux-port
  and FreeRTOS POSIX-port smokes: of two equal blocks the lower is grown
  32 → 4096 and the higher holds a guard pattern; the probe first checks the
  guard is inside the grow's reach (else it would be vacuous). Before: longest
  guard run in the grown tail = 32 on ThreadX and FreeRTOS heap_4 (FAIL);
  after: 0 on ThreadX, heap_4 and heap_3.
* The FreeRTOS smoke now builds TWICE, heap_4 and heap_3, because the port's
  allocator differs between them. It had not linked at all on `main`:
  it forced heap_3 without telling the port, so `xPortGetFreeHeapSize` was
  undefined — what issue 1720 (no lane runs these smokes) costs.
* `zpico-alloc`
  `foreign_realloc_is_refused_and_never_reads_past_the_block`: before, 240
  guard bytes from past a 16-byte foreign block were copied; after, NULL.
