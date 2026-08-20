---
id: 733
title: "Embedded Cyclone on FreeRTOS: type descriptors register through `__attribute__((constructor))`, and bare-metal never runs it"
status: resolved
type: bug
severity: medium
area: freertos
related: [phase-370, issue-0233, issue-0048, issue-0195]
resolved_in: "phase-370 W4 — the #195 `.init_array` pattern, brought to the FreeRTOS family"
---

# 0733 — the embedded Cyclone lane's remaining wall, after the ones phase-370 W4 cleared

phase-370 W4 set out to revive a single C `cyclonedds` × `mps2-an385-freertos`
fixture. The lane went from **does not compile** to **boots, brings up the
network, creates a participant** — and then:

```
$ qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
    -semihosting-config enable=on,target=native -kernel c_talker
Network ready
[nros] examples/qemu-arm-freertos/c/talker/src/main.c:105
       nros_publisher_init(&app.publisher, &app.node,
         std_msgs_msg_string_get_type_support(), "/chatter") -> -1
```

No assert, no fault, no diagnostic — a bare `-1` from writer creation. Session
creation succeeded, so the domain and participant exist.

## What W4 already cleared (all landed)

Reaching this point took nine fixes, and none of them was specific to a new
board — each was a seam nothing had walked:

* Three `std::` C-name references that a cross libc does not alias
  (`getenv`, `strtoull`, and `calloc`/`free`). Phase 203 had recorded this for
  ONE symbol on ONE libc; newlib on arm-none-eabi aliases a different subset,
  which is what made it a class rather than a site.
* Those `calloc`/`free` were on TRANSIENT SAMPLES, i.e. the hazard
  `docs/reference/cyclonedds-known-limitations.md` states outright: "transient
  samples use `ddsrt_{malloc,calloc,free}`, never libc — RTOS heap is separate".
  `subscriber.cpp` beside it already followed the rule; `service.cpp` did not.
* `-ffreestanding` means `getenv` is not declared AT ALL, which is correct: the
  image has no environment. Now `env_lookup`, one spelling for the three sites.
* The lwIP per-thread semaphore was allocated BEFORE `tcpip_init` created the
  pools it comes from — so the app task's TLS slot held nothing and the first
  socket call asserted `sem != NULL`. Latent because zenoh-pico opens its
  sockets from its own tasks, which take a different path; Cyclone creates its
  endpoints from the APP task.

## RETRACTED: "the ddsrt thread fix is not the cause"

This issue originally said that adding `lwip_socket_thread_init()` to ddsrt's
FreeRTOS `thread_start_routine` "changes nothing, measured". **That measurement
was wrong**, and it was wrong in the way this repo already warns about: it was
taken on an INCREMENTAL build. Reverting the file in the vendored submodule did
not recompile the cyclonedds subproject, so both runs executed the same image.

Re-measured from CLEAN build directories in both directions:

| `network_glue.c` order | ddsrt thread fix | result |
| --- | --- | --- |
| either | **no** | `lwIP ASSERT: sem != NULL`, then `-1` |
| either | **yes** | no assert, then `-1` |

So the ddsrt change IS the fix for the assert. It is fork commit `99cfac88`
(`docs/reference/cyclonedds-fork-delta.md` §5), pushed to `origin/nano-ros` and
now the superproject pin.

The same table retires the other half of the original story: the
`network_glue.c` reordering of `lwip_socket_thread_init()` after `tcpip_init()`
is neither necessary nor sufficient, and its stated rationale was false —
`sys_sem_new()` on this port is `xSemaphoreCreateBinary()`, which takes FreeRTOS
heap, not lwIP's memp pools. It has been reverted.

## The actual cause of the `-1`

Instrumenting `publisher_create`'s early returns:

```
Network ready
W733:no-desc
[nros] … nros_publisher_init(…) -> -1
```

`find_descriptor(eff_type)` returns `nullptr` — the type-descriptor registry is
EMPTY. And the registration TU that `nros_rmw_cyclonedds_idlc_compile()`
generates (`NrosRmwCycloneddsTypeSupport.cmake`) registers like this:

```c
void register_<stem>_<n>(void) {
    nros_rmw_cyclonedds_register_descriptor("<type>", &<desc>);
}

__attribute__((constructor))
static void register_<stem>_<n>_constructor(void) { register_<stem>_<n>(); }
```

`.init_array` is **not walked on bare-metal/RTOS** — this tree says so outright,
in `packages/api/nros-c/src/rmw_backend.rs`: "`.init_array` is not walked on
bare-metal/RTOS — the #48 hazard". RMW BACKEND registration was moved off ctors
onto a generated strong definition for exactly this reason (phase-249 P3).
Descriptor registration never was, because no embedded Cyclone image had ever
run far enough to need a descriptor.

Note the generator already emits the registrar as a NAMED, non-static function
beside the constructor — the explicit-call path exists and has no caller.

## Resolved — option 2, and the measurement that chose it

Two fixes were on the table: an aggregate strong registrar (the phase-249 P3
shape used for RMW BACKENDS), or making the board walk `.init_array`.

The first comparison favoured the registrar on the grounds that the register
objects "are never pulled from the archive at all". **That was wrong**, and
measuring the link said so:

```
$ grep -o whole-archive link.txt | wc -l              6
$ grep -oE '[a-z_]+__cyclonedds_ts' link.txt          builtin_interfaces, std_msgs
$ nm c_talker | grep -cE '_desc$|_desc '              9
```

The type-support archives are ALREADY whole-archived on bare metal —
`NrosRmwCycloneddsTypeSupport.cmake` has a branch for exactly
`CMAKE_SYSTEM_NAME STREQUAL "Generic"`, and its comment names this symptom:
"the descriptor static-init ctors get GC'd → `find_descriptor -> nullptr ->
register_subscription -> -1`". The descriptors were in the image. Only the ctor
BODIES were dropped, because the linker script had no `.init_array` output
section. (`nm | grep -c register_.*_constructor → 0` misled the first reading:
those are `static` and have no external symbol.)

So the machinery to feed ctors was already there and deliberate, and issue #195
had already solved the same problem the same way for
`nros-board-threadx-qemu-riscv64` — a `.init_array` KEEP block plus an explicit
walk. The FreeRTOS family was simply the one embedded Cyclone board that never
got it. The registrar would have been a SECOND registration mechanism beside
#195's.

## What landed

* `config/nros-freertos-cortex-m.ld` — an `.init_array` output section with
  `KEEP` and `PROVIDE(__init_array_start/end)`. Shared across the FreeRTOS
  Cortex-M boards, so no per-board edit.
* `c/freertos_hooks.c` — `nros_board_freertos_run_init_array()`, idempotent,
  called from the C/C++ lane's `freertos_c_entry.c` and the Rust lane's
  `freertos_boot_bringup`, before anything can look a descriptor up.
* `c/freertos_hooks.c` — `__dso_handle`, `__cxa_atexit`, `__cxa_finalize`,
  `_fini`. **Not incidental:** keeping `.init_array` retains C++ objects with
  static storage duration, and one with a DESTRUCTOR registers it through
  `__cxa_atexit(dtor, obj, &__dso_handle)`. A `-nostartfiles` image links no
  crt, so the C++ workspace entry failed with `undefined reference to
  __dso_handle` / `_fini` — a C++ ABI message that names nothing about the
  linker script that caused it. No-ops are correct rather than a workaround:
  this image never exits, so a static destructor could only run at a point that
  does not exist.

## Proof

```
$ readelf -S c_talker | grep init_array
  [ 2] .init_array   INIT_ARRAY  000d8f98 0d9f98 000094  ...   (37 ctors)

$ qemu-system-arm -machine mps2-an385 -kernel c_talker
Network ready
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
...

$ qemu-system-arm -machine mps2-an385 -kernel c_listener
Network ready
Subscriber created for topic: /chatter
```

Writer AND reader create — the reader was #195's original symptom
(`nros_executor_add_subscription -> -1`).

Regression surface, all green: `just freertos build-fixtures` (the whole lane,
Rust + C + C++, which is what caught the `__dso_handle` half),
`entry_e2e` freertos zenoh cells (3 ran, 0 failed — ctors now run on zenoh
images too, which is the behaviour change), `check-rmw-cyclonedds`, and the
`freertos_posix` cells.

Both are real; picking between them is a design call, not a detail, which is why
this is filed rather than patched.

## Not the cause (kept from the investigation)

* The heap. `NROS_FREERTOS_HEAP_KB` defaults to 3 MB and the failure is a
  registry miss, not an allocation failure.
* Tracing. The earlier note that diagnostics were the blocker is superseded —
  four temporary `console_write` probes in `publisher_create` located this in one
  boot. A compile-time Cyclone trace knob would still be useful, and is no longer
  a prerequisite.

Issue 0233 tracks the older restore-vs-carve question for these cells; this is
the concrete blocker if the answer is restore.
