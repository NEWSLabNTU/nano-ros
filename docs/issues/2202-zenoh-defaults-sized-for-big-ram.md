---
id: 2202
title: "An undeclared zenoh C image on a 520 KiB part is ~400 KB of RAM: pool and heap defaults are sized for big-RAM targets"
status: open
type: enhancement
area: [memory, zephyr, rmw-zenoh]
severity: medium
found: 2026-11-01
related: [2201, 1122, 1125, 1324, 1033, phase-524, phase-530]
---

**Home:** [phase-524](../roadmap/phase-524-emulator-twins.md)
— W3. These are the RAM defaults every small adopter board falls back to; issue 2194 (resolved) added the board rung that overrides them.

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
  `book/src/guide/contracts/measure-static-memory.md` says it is filed under
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

## Measured on in-tree boards (2026-11-10)

`just mem-report --json` (with the item 5 fix below) on Zephyr 3.7 builds of
`examples/zephyr/c/talker`, and of the C++ one on mps2. Each board has three
variants:

* **A**: a scratch copy of the leaf with `entities = "defaults"`. This is the
  undeclared fallback, written down.
* **census**: the tracked leaf as it is (no `entities`). RFC-0120 D4 sizes a
  cross C/C++ leaf from its census by default. This is the `--from-leaf` road.
* **B**: a scratch copy with `entities = ["publisher:std_msgs/msg/String:/chatter", "timer"]`.

The mps2 build is the `build-cortex-m-c-talker-zenoh` fixture: TCP, with the
board's `prj.conf`. The Nucleo build is `[image.nucleo_serial]`: zenoh over
USART3, on a 512 KiB SRAM part.

| pool (symbol) | mps2 A | mps2 census | mps2 B | nucleo A | nucleo census | nucleo B | mps2 C++ A | mps2 C++ census |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| executor storage (`GlobalStorageHolder<0>::storage`) | 83,880 | 6,072 | 6,072 | 83,880 | 6,072 | 6,072 | 83,880 | 6,768 |
| `zephyr_heap::HEAP` | 67,280 | 67,280 | 67,280 | 67,280 | 67,280 | 67,280 | 67,280 | 67,280 |
| `kheap__system_heap` | 131,072 | 131,072 | 131,072 | 16,384 | 16,384 | 16,384 | 131,072 | 131,072 |
| `USER_SERVICE_INBOX` | 33,412 | 4,180 | 4,180 | 33,412 | 4,180 | 4,180 | 33,412 | 4,180 |
| `SMALL_PAYLOADS` | 32,768 | 4,096 | 4,096 | 32,768 | 4,096 | 4,096 | 32,768 | 4,096 |
| `LARGE_PAYLOADS` | 131,072 | - | - | - | - | - | 131,072 | - |
| `nros_thread_stacks` | 32,896 | 32,896 | 32,896 | 32,896 | 32,896 | 32,896 | 32,896 | 32,896 |
| `g_sessions` (includes the graph cache on IP) | 84,784 | 82,752 | 82,752 | 19,208 | 17,176 | 17,176 | 84,784 | 82,752 |
| `z_main_stack` | 131,136 | 131,136 | 131,136 | 16,448 | 16,448 | 16,448 | 131,136 | 131,136 |
| `static_subscriber_storage::SLOTS` | 8,192 | - | - | 8,192 | - | - | 8,192 | - |
| `MESSAGE_INFO_TABLE` | 4,096 | 4,096 | 4,096 | 4,096 | 4,096 | 4,096 | 4,096 | 4,096 |
| `TL_SLOTS` | 2,688 | 1,344 | 1,344 | 2,688 | 1,344 | 1,344 | 2,688 | 2,688 |
| RAM (writable allocated sections) | 810,681 | 527,417 | 527,417 | 353,276 | 201,088 | 201,088 | 809,693 | 529,141 |

What this shows:

* **The census road and a hand declaration give the same image.** On both
  boards they match byte for byte. The tracked C leaf is therefore already
  declared-sized, so the "Also found" note that it "takes the same
  undeclared defaults" is out of date since RFC-0120 D4. Direction item 6
  needs no `entities` line to move a byte. A hand list would only skip the
  host census build.
* **On the Nucleo, declaring saves 152,188 B: from 353,276 (67 % of 512 KiB)
  to 201,088 (38 %).**
* **What stays big whatever the image declares:**
  * the nano-ros heap, 67,280 B (`NROS_ZEPHYR_HEAP_SIZE` 64 KiB);
  * the kernel heap: 128 KiB on mps2 (board `prj.conf`), 16 KiB on the Nucleo;
  * `nros_thread_stacks`: 2 x 16 KiB;
  * the main stack: 128 KiB on mps2, 16 KiB on the Nucleo;
  * the 4 KiB message-info table;
  * the graph cache, on an IP link. The cache sits inside `g_sessions`
    (`zpico.c` `graph_cache_t graph_cache`), not in a symbol of its own. That
    is why `g_sessions` is 82,752 B over TCP on mps2 but 17,176 B on the
    serial Nucleo, which derives `NROS_ZENOH_GRAPH_DISCOVERY=0`.
* **C++ leaf, census road:** `TL_SLOTS` stays at two slots (2,688 B), where
  the C census and the workspace C++ image get one. Not diagnosed.

Model road (direction item 6's other half): the workspace images
`pubsub_bringup:zephyr_talker{,_xrce,_cyclonedds}` in `examples/workspaces/{c,cpp}`.
They have native_sim fixture rows only. Each comes out at the declared size
with no hand declaration, with the same per-pool bytes as the census-sized
native_sim leaf (zenoh: `USER_SERVICE_INBOX` 4,232, `SMALL_PAYLOADS` 4,096,
no `LARGE_PAYLOADS`, no subscriber `SLOTS`, `TL_SLOTS` 1,368). The executor
storage is 13,528 B, against 13,720 for the C leaf and 14,432 for the C++
leaf. No model-road gap was found.

**Item 5, resolved here.** The executor storage is a template's static data
member. `nm` prints it `V` (weak object) on the arm builds and `u` (GNU unique)
on native_sim. Neither letter was in any of `mem-report`'s type sets, so the
symbol was in no total: not the owner table, not the top-RAM list, and not the
storage roles. The `[executor storage]` role regex already matched it, but the
symbol never reached the regex.

`nros-mem-report.py` now resolves `V`/`v`/`u` from the flags of the section
that defines the symbol (`st_shndx`, from `readelf -s`). It does not use the
symbol's address: a native_sim `zephyr.elf` is a relocatable ELF, with every
section at address 0. On the Nucleo A image, the unattributed gap goes from
84,464 B (23.9 %) to 580 B, and the storage is listed under
`[executor storage]` at 83,880 B.
