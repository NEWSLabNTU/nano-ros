---
id: 1566
title: "A C component's publisher buffer is a literal 560 bytes, and
  nros_cpp_publisher_create writes NROS_PUBLISHER_SIZE + 8 into it — 640 on
  Zephyr native_sim, so the overrun corrupted the platform heap"
status: resolved
type: bug
area: c-api, zephyr, memory
severity: high
found: 2026-09-29
related: [issue-1551, issue-0282, issue-0268]
---

# A C component's publisher buffer is 80 bytes short on Zephyr

## Symptom (measured)

Found while fixing issue 1551. Once the tiered `realtime-c` Zephyr image
stopped running out of heap at boot, it got as far as tier 1's first entity
declare and died with SIGSEGV, `native_sim/native/64` under `rmw_zenohd`:

```
Thread 11 "zephyr.exe" received signal SIGSEGV, Segmentation fault.
_z_list_len (xs=<optimized out>) at zenoh-pico/src/collections/list.c:102
#2  _z_hashmap_len (map=0x5616a0 <nros_platform::zephyr_heap::HEAP+1128>)
#6  _z_cache_declaration (zs=0x561530 <nros_platform::zephyr_heap::HEAP+760>, …)
…
#23 nros_cpp::publisher::nros_cpp_publisher_create () at nros-cpp/src/publisher.rs:161
#24 ctrl_configure (self=0x560dc0 <__nros_c_inst_ctrl_pkg>, …)
```

The zenoh session lives in the platform heap, and its hashmap had been
overwritten.

## Cause

`nros/component.h` sizes a C component's publisher buffer with a literal:

```c
#define NROS_C_PUBLISHER_STORAGE_SIZE 560
```

`nros_cpp_publisher_create` takes no size argument and writes a
`CppPublisher`, whose size its own `const` assertion fixes at
`NROS_PUBLISHER_SIZE + sizeof(void*)`. On this build the per-build header says
`NROS_PUBLISHER_SIZE 632`, so the write is 640 bytes into a 560-byte field.

The linker placed the two component instances directly before the heap:

```
0000000000560dc0 0000000000000238 b __nros_c_inst_ctrl_pkg
0000000000561000 0000000000000238 b __nros_c_inst_telem_pkg
0000000000561238 0000000000010b48 b nros_platform::zephyr_heap::HEAP
```

`telem_pkg`'s publisher write ran 80 bytes past its 568-byte instance into the
rlsf arena's first block. Everywhere else the same overrun lands in some
neighbouring `.bss` object and says nothing, so every C talker in the tree
carries it.

The literal was stale even for the build it came from. The NuttX snapshot
header states `NROS_PUBLISHER_SIZE 560`, so it was 8 bytes short there too:
the `+ sizeof(void*)` monitor cell was never counted. This is the issue-0282
class. `component.h` already derives `NROS_C_ACTION_{SERVER,CLIENT}_STORAGE_SIZE`
from the per-build header, and the publisher never got the same treatment.

## Resolution

`NROS_C_PUBLISHER_STORAGE_SIZE` is `(NROS_PUBLISHER_SIZE + sizeof(void*))`
whenever the generated header is reachable, and it keeps the literal only as
the fallback for a build with no header, exactly like its action siblings.
After the fix `realtime-c` boots both tiers (see issue 1551's resolution for the
run).

## Not done

- `NROS_C_SERVICE_CLIENT_STORAGE_SIZE` is still a literal (4632). It is larger
  than `NROS_SERVICE_CLIENT_SIZE` (576) on this build, and it was not checked
  against what `nros_cpp_service_client_create` actually writes.
- Nothing gates a C storage literal against the size its consumer writes. A
  `static_assert` in nros-cpp is not possible because the consumer takes no
  size. The durable fix is a size parameter on the create call, which is an
  ABI change and was not attempted here.
