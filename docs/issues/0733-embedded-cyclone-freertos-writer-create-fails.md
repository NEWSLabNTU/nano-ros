---
id: 733
title: "Embedded Cyclone on FreeRTOS now builds and boots, and `nros_publisher_init` still returns -1"
status: open
type: bug
severity: medium
area: freertos
related: [phase-370, issue-0233]
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

## What was tried and is NOT the cause

`ddsrt`'s FreeRTOS `thread_start_routine` does not call
`lwip_socket_thread_init()` for the threads Cyclone creates itself, which looks
like the same defect one layer down. It was implemented in the vendored fork and
measured: **the failure is byte-identical with and without it**, so it is not
what this is, and no fork commit was made. (It may still be needed once writers
exist and those threads do socket I/O — but landing an untested fork change on
that guess is how a fork delta grows entries nobody can justify.)

## Where to look next

* `dds_create_topic` / the `sertype_min` registration, which is the step between
  a working participant and a failing writer.
* The fixed-pool heap budget (`kEmbeddedCycloneConfig`, Phase 177.22) — the
  default is 3 MB (`NROS_FREERTOS_HEAP_KB`), which does not LOOK tight, but
  nothing has measured Cyclone's actual embedded working set.
* Diagnostics are the immediate problem: `CYCLONEDDS_URI` cannot be used to turn
  on tracing, because `env_lookup` correctly answers `nullptr` on a freestanding
  target. A compile-time trace knob is probably the prerequisite for any further
  progress here.

## Why this is filed rather than finished

phase-370 W4 is a stretch item its own doc says "may split out". The build half
is landed and independently valuable — it is what makes the lane debuggable at
all. The remaining step needs Cyclone-level tracing on a target with no
environment, which is its own piece of work.

Issue 0233 tracks the older restore-vs-carve question for these cells; this is
the concrete blocker if the answer is restore.
