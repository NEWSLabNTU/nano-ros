# Phase-581 W5 - before/after measurements of the batch-1 renames

Measured 2026-11-20 in the `ros2` distrobox (Ubuntu 22.04, ROS 2 Humble), per
[phase-581 W5](../phase-581-ros-env-vars.md). "Before" is the W4 tip
(`b9923d063d`), checked out as a scratch worktree under `/mnt/wd/nt/` and
removed afterwards; "after" is the W5 working tree. Each side ran its own
`just setup-cli` (and, before, `just setup-launch-resolve`), then built the
same three images with no knob stated, so every value below is a default or a
derivation. Scratch drivers and raw `nm` dumps lived in `/mnt/wd/nt/p581w5/`.

| Platform | Command | Measured artifact |
| --- | --- | --- |
| Zephyr 3.7 | `just zephyr build-one c/talker` (`nros_zephyr_talker_c_bringup:zephyr_zenoh`, `native_sim/native/64`) | `<build>/zephyr/.config`, `zephyr.exe` |
| FreeRTOS | `nros build mps2-an385-freertos` in `examples/mps2-an385-freertos/rust/talker` | `mps2_an385_freertos_entry` (thumbv7m, release) |
| ThreadX | `nros build rv-virt-threadx` in `examples/rv-virt-threadx/rust/talker` | `librv_virt_threadx_talker.a` and the board's `nros_threadx_hooks.o` (the leaf's `nros build` produces the staticlib; the image link is the cmake road's) |

## `.config` (Zephyr)

1909 lines on both sides, 89 `CONFIG_NROS_*` symbols after. 19 of them are
renamed by W5; mapping each old symbol to its new one, the two sets are
identical, symbol for symbol and value for value (0 differences):

| Before | After | Value |
| --- | --- | --- |
| `CONFIG_NROS_ZEPHYR_HEAP_SIZE` | `CONFIG_NROS_MEMORY_HEAP_BYTES` | 65536 |
| `CONFIG_NROS_EXECUTOR_ARENA_SIZE` | `CONFIG_NROS_EXECUTOR_ARENA_BYTES` | 0 |
| `CONFIG_NROS_EXECUTOR_BACKING_U64S` | `CONFIG_NROS_EXECUTOR_BACKING_BYTES` | -1 (derive) |
| `CONFIG_NROS_BATCH_UNICAST_SIZE` | `CONFIG_NROS_ZENOH_BATCH_UNICAST_BYTES` | 1024 |
| `CONFIG_NROS_FRAG_MAX_SIZE` | `CONFIG_NROS_ZENOH_FRAG_MAX_BYTES` | 2048 |
| `CONFIG_NROS_GET_POLL_INTERVAL_MS` | `CONFIG_NROS_ZENOH_GET_POLL_INTERVAL_MS` | 10 |
| `CONFIG_NROS_GET_REPLY_BUF_SIZE` | `CONFIG_NROS_ZENOH_GET_REPLY_BUFFER_BYTES` | 4096 |
| `CONFIG_NROS_GRAPH_CACHE_SIZE` | `CONFIG_NROS_ZENOH_GRAPH_CACHE_BYTES` | 65536 |
| `CONFIG_NROS_LOG_BUFFER_SIZE` | `CONFIG_NROS_LOG_BUFFER_BYTES` | 256 |
| `CONFIG_NROS_MAX_BYTE_ARRAY_LEN` | `CONFIG_NROS_PARAMS_MAX_BYTE_ARRAY_BYTES` | -1 |
| `CONFIG_NROS_MAX_PARAM_CONSTRAINTS_LEN` | `CONFIG_NROS_PARAMS_MAX_CONSTRAINTS_BYTES` | 0 |
| `CONFIG_NROS_MAX_PARAM_DESCRIPTION_LEN` | `CONFIG_NROS_PARAMS_MAX_DESCRIPTION_BYTES` | 256 |
| `CONFIG_NROS_MAX_PARAM_NAME_LEN` | `CONFIG_NROS_PARAMS_MAX_NAME_BYTES` | -1 |
| `CONFIG_NROS_MAX_STRING_VALUE_LEN` | `CONFIG_NROS_PARAMS_MAX_STRING_VALUE_BYTES` | -1 |
| `CONFIG_NROS_PARAM_SERVICE_BUFFER_SIZE` | `CONFIG_NROS_EXECUTOR_PARAM_SERVICE_BUFFER_BYTES` | -1 |
| `CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE` | `CONFIG_NROS_SUBSCRIBER_BUFFER_BYTES` | -1 |
| `CONFIG_NROS_SUBSCRIBER_LARGE_SIZE` | `CONFIG_NROS_ZENOH_SUBSCRIBER_LARGE_BYTES` | -1 |
| `CONFIG_NROS_SUBSCRIBER_RING_DEPTH` | `CONFIG_NROS_ZENOH_SUBSCRIBER_RING_DEPTH` | 4 |
| `CONFIG_NROS_SUBSCRIPTION_BUFFER_SIZE` | `CONFIG_NROS_EXECUTOR_SUBSCRIPTION_BUFFER_BYTES` | -1 |

FreeRTOS and ThreadX have no `.config`; their knobs reach the build as
environment and `-D` values, so the static storage below is the comparison.

## `nm` sizes of the affected pools

| Image | Symbol | Before | After |
| --- | --- | --- | --- |
| Zephyr | `nros_platform::zephyr_heap::HEAP` (`memory.heap_bytes`) | 68448 | 68448 |
| FreeRTOS | `ucHeap` (`memory.heap_bytes`, KiB in `FreeRTOSConfig.h`; holds the app stack) | 720896 | 720896 |
| ThreadX | `byte_pool_storage` (`threadx.byte_pool_bytes`, derived; holds the app stack) | 851968 | 851968 |

Every sized `.data`/`.bss` symbol, compared as a multiset of (name with the
crate hash removed, size):

| Image | Symbols | Bytes before | Bytes after | Differences |
| --- | --- | --- | --- | --- |
| Zephyr `zephyr.exe` | 439 | 422777 | 422777 | 0 |
| FreeRTOS entry | 252 | 915878 | 915878 | 0 |
| ThreadX staticlib | 522 | 1407337 | 1407337 | 0 |
| ThreadX `nros_threadx_hooks.o` | 12 | 854968 | 854968 | 0 |

FreeRTOS `size`: `data` 5128 and `bss` 919012 on both sides; `text` is
507708 before and 509228 after (+1520). The difference is read-only strings,
not code: the panic locations embed the checkout's absolute path, which is 40
characters longer for the W5 worktree than for the scratch one, and messages
that name a knob now name the new spelling (`raise NROS_MEMORY_HEAP_BYTES`,
`NROS_EXECUTOR_ARENA_BYTES`).

## Toolchains

All three were present in the box (`arm-none-eabi-gcc` 13.2 from the SDK
store, `riscv64-unknown-elf-gcc` from the system, the Zephyr 3.7 workspace
from the store). Nothing was skipped.
