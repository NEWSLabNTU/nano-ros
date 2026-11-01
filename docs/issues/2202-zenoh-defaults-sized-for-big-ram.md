---
id: 2202
title: "An undeclared zenoh C image on a 520 KiB part is ~400 KB of RAM: pool and heap defaults are sized for big-RAM targets"
status: open
type: enhancement
area: [memory, zephyr, rmw-zenoh]
severity: medium
found: 2026-11-01
related: [2201, 1122, 1125, 1324, 1033]
---

## Problem

A C zenoh talker (one publisher, one timer) on `xiao_rp2350` (RP2350, 520 KiB
SRAM, serial link) uses 401,575 B of RAM (75 %) even with issue 2201's
`max_large_subscribers = 0` workaround. Without the workaround the link fails.
Most of that is defaults that only make sense on a host-scale target. Some of
them are the configured value an UNDECLARED image falls back to, and some are
constants that no declaration moves.

This issue measures each pool, gives its size, and says where its value comes
from. Changing the defaults is a separate PR. Issue 2201 fixed only the
derivation of `NROS_MAX_LARGE_SUBSCRIBERS`.

## Measurement

`just mem-report` and `arm-none-eabi-nm -S` on two builds of the same app
(`/mnt/p5plus/aeon/nros-xiao-ws/apps/talker-c`, Zephyr 4.4, gnuarmemb 13.2):

* **A**: the spike's ELF: no `entities`, `max_large_subscribers = 0` stated.
  RAM 401,575 B.
* **B**: the same app with `[[component]] entities =
  ["publisher:std_msgs/msg/String:/chatter", "timer"]` and no knob stated.
  RAM 249,387 B.
* **(none)**: no `entities` and no knob. Fails to link, `RAM overflowed`. The
  extra term is `LARGE_PAYLOADS` 131,072 B (2 x 4 x 16,384, crate default
  `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS`).

| pool (symbol) | A | B | where the value comes from | moved by declaring entities? |
|---|---:|---:|---|---|
| executor storage (`rclcpp::Node::GlobalStorageHolder<0>::storage`) | 83,880 | 6,072 | executor arena/slot defaults (`NROS_EXECUTOR_*`, nros-node build.rs) when the entity inventory refuses | yes |
| `nros_platform::zephyr_heap::HEAP` | 67,280 | 67,280 | `NROS_ZEPHYR_HEAP_SIZE`, `zephyr/Kconfig` `default 65536` (C image) plus 512 B slab and alignment. A C talker measured a 15,152 B peak (the Kconfig help) | no |
| `kheap__system_heap` | 65,620 | 65,620 | `CONFIG_HEAP_MEM_POOL_SIZE=65536` in the board's `prj.conf` and the app's `prj-zenoh.conf`. The Kconfig help says nano-ros no longer allocates from it. Only Zephyr drivers (native_sim's NSOS) need it to be > 0 | no |
| `USER_SERVICE_INBOX` | 33,408 | 4,176 | `NROS_SERVICE_INBOX_BYTES`/`_DEPTH` x `NROS_ZENOH_MAX_SESSIONS` x `NROS_ZENOH_MAX_QUERYABLES` (zpico literal 8 when undeclared) | yes |
| `SMALL_PAYLOADS` | 32,768 | 4,096 | `NROS_ZENOH_MAX_SUBSCRIBERS` (zpico literal 8) x ring depth x `NROS_SUBSCRIBER_BUFFER_SIZE`. Even with 0 subscriptions declared, one block's worth stays | mostly |
| `nros_thread_stacks` | 32,768 | 32,768 | `NROS_ZEPHYR_TASK_SLOTS` (2 when `NROS_ZENOH_TX_BATCH`) x `NROS_ZEPHYR_TIER_STACK_SIZE` `default 16384` | no |
| `g_sessions` | 19,208 | 17,176 | zenoh-pico session, sized by `NROS_ZENOH_MAX_{PUBLISHERS,SUBSCRIBERS,QUERYABLES,LIVELINESS,PENDING_GETS}` | slightly |
| `z_main_stack` | 16,384 | 16,384 | `CONFIG_MAIN_STACK_SIZE=16384`, board `prj.conf` | no |
| `static_subscriber_storage::SLOTS` | 8,192 | 0 | `NROS_RMW_SUBSCRIBER_SLOTS` x slot bytes | yes |
| `nros_rmw_cffi::MESSAGE_INFO_TABLE` | 4,096 | 4,096 | `NROS_RMW_MESSAGE_INFO_SLOTS` | no |
| `TL_SLOTS` | 2,688 | 1,344 | `NROS_ZENOH_MAX_TL_PUBLISHERS` x `NROS_ZENOH_TL_RETAIN_*` | yes |
| graph cache | not linked | not linked | `NROS_GRAPH_CACHE_SIZE` `default 65536` (`zephyr/Kconfig`). The serial image derives `NROS_ZENOH_GRAPH_DISCOVERY=0`, so the cache is compiled out. A UDP/TCP image on the same part pays the 64 KiB | no |

What is left once the image declares (B) is about 200 KB of constants that no
declaration moves. These are the two 64 KiB heaps, 48 KiB of stacks, and the
4 KiB message-info table. A 64 KiB graph cache joins them on any IP link.

## Also found

* **`mem-report` does not attribute the executor storage on this image.** In A,
  83,880 of its 83,895 "unattributed" bytes are
  `rclcpp::Node::GlobalStorageHolder<0>::storage`. That symbol is weak (`nm`
  type `V`) and lives in the `rclcpp::` namespace.
  `book/src/internals/measuring-static-memory.md` says it is filed under
  `[executor storage]` as `Node::GlobalStorageHolder<0>::storage`. It is not
  filed there, so the biggest movable pool on the image does not appear in the
  report's top list.
* The in-tree `examples/zephyr/c/talker` zenoh image also declares no
  `entities`, so it takes the same undeclared defaults as A. The XRCE image in
  that file states its caps for the same reason (issue 1033).

## Direction (for the follow-up PR)

1. The `CONFIG_HEAP_MEM_POOL_SIZE` default on Zephyr boards where nothing
   nano-ros owns allocates from the kernel heap: measure the real kernel-heap
   peak and state a small value per board. Keep native_sim > 0.
2. `NROS_ZEPHYR_HEAP_SIZE` for C images: 64 KiB against a measured 15 KiB
   peak. Consider deriving it from the declaration, or document a small-part
   value.
3. The `NROS_GRAPH_CACHE_SIZE` default (its own help text calls it 20 % of the
   board's SRAM).
4. The stacks: `NROS_ZEPHYR_TIER_STACK_SIZE` x 2 slots on a single-tier talker.
5. Fix `mem-report`'s attribution of the weak `rclcpp::` executor storage.
6. Declare `entities` in the in-tree Zephyr C/C++ talker examples, so the
   examples show the declared road.
