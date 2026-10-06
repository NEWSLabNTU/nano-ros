---
id: 1719
title: "ThreadX and FreeRTOS `nros_platform_realloc` copy the NEW size out of the OLD block — a grow reads past the end of the allocation"
status: open
type: bug
severity: low
area: [platform, threadx, freertos, memory]
related: [1717]
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
