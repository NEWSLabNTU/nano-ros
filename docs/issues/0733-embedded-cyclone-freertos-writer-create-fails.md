---
id: 733
title: "Embedded Cyclone on FreeRTOS: type descriptors register through `__attribute__((constructor))`, and bare-metal never runs it"
status: open
type: bug
severity: medium
area: freertos
related: [phase-370, issue-0233, issue-0048]
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

So the ddsrt change IS the fix for the assert, and it is now committed in the
fork (`99cfac88`, see `docs/reference/cyclonedds-fork-delta.md` §5) — local
only, pending a maintainer push.

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

## The two ways to fix it

1. **Follow phase-249 P3.** `nros_rmw_cyclonedds_add_idl_library` knows every
   `register_<stem>_<n>` it generated, so it can emit one aggregate strong
   definition, called once from session setup. Matches how backend registration
   already works here, and keeps `.init_array` irrelevant.
2. **Make the board walk `.init_array`.** The mps2 FreeRTOS board links
   `-nostartfiles`, so nothing runs the array a normal C runtime would. A walk in
   the board's startup fixes every ctor-based registration at once rather than
   this one — but it re-adopts the mechanism #48 moved away from, so it should
   only be chosen deliberately.

Both are real; picking between them is a design call, not a detail, which is why
this is filed rather than patched.

## Not the cause

* The heap. `NROS_FREERTOS_HEAP_KB` defaults to 3 MB and the failure is a
  registry miss, not an allocation failure.
* Tracing. The earlier note that diagnostics were the blocker is superseded —
  four temporary `console_write` probes in `publisher_create` located this in one
  boot. A compile-time Cyclone trace knob would still be useful, and is no longer
  a prerequisite.

Issue 0233 tracks the older restore-vs-carve question for these cells; this is
the concrete blocker if the answer is restore.
