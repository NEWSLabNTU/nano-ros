---
id: 1093
title: "zenoh-pico 1.8.0 compiles an IPv6 endpoint helper for the lwIP arm, so every FreeRTOS fixture fails on an undeclared `INET6_ADDRSTRLEN`"
status: resolved
type: bug
area: build, rmw
severity: high
found: 2026-09-05
related: [1075, 1080, 1082, phase-415, 0507]
---

# Upstream assumed every lwIP build has IPv6

## Symptom

Every FreeRTOS fixture fails in the vendored zenoh-pico:

```
zenoh-pico/src/system/common/platform.c:91:13: error: 'INET6_ADDRSTRLEN'
undeclared (first use in this function); did you mean 'INET_ADDRSTRLEN'?
error: failed to run custom build command for `zpico-sys v0.5.0`
```

## Cause

`platform.c:74` compiles both endpoint helpers for one arm:

```c
#if defined(ZENOH_WINDOWS) || defined(ZENOH_LINUX) || defined(ZENOH_MACOS) || \
    defined(ZENOH_BSD) || defined(ZENOH_FREERTOS_LWIP)
```

`_z_ipv4_port_to_endpoint` uses `INET_ADDRSTRLEN`, which lwIP always defines.
`_z_ipv6_port_to_endpoint` uses **`INET6_ADDRSTRLEN`, which lwIP defines only
under `LWIP_IPV6`** — and nano-ros ships `LWIP_IPV6 0`
(`packages/boards/nros-board-freertos/config/lwipopts.h:45`).

So upstream's arm treats "lwIP" as implying IPv6. Ours is a v4-only build, which
is a supported lwIP configuration.

The helper arrived with upstream `04c6b1c7` (*"Connectivity events and API"*,
#1159), which came in on the **1.8.0 bump — phase-415**, on 2026-09-04.

## Why it took a day to surface

The FreeRTOS fixtures need a fixture build, and the Zephyr family builds ahead of
them in the same lane. Zephyr had been failing for three unrelated reasons —
issues **1075** (link), **1080** (compile) and **1082** (a stale Kconfig value) —
so the lane never reached FreeRTOS at all. Four failures stacked in one lane,
each hiding the next; this was the fourth.

## Fix

A fallback beside the existing lwIP include, on the fork's `nano-ros` patch
branch (`66556c99`):

```c
#ifndef INET6_ADDRSTRLEN
#define INET6_ADDRSTRLEN 46
#endif
```

46 is the standard value (RFC 4291 text form + NUL).

**Chosen over excising the helper from the lwIP arm.** On a v4-only stack
`inet_ntop(AF_INET6, ...)` returns NULL, which the function *already* treats as
`_Z_ERR_GENERIC` — so the fallback keeps the honest runtime answer, needs no
call-site surgery, and diverges less from upstream. Removing the function would
mean touching every caller and widening the fork delta, which
`docs/reference/cyclonedds-fork-delta.md`'s sibling discipline argues against.

Upstream-worthy in this shape: the guard belongs beside the include either way,
and a v4-only lwIP is a configuration upstream supports elsewhere.

## Not covered

* Whether other 1.8.0 arrivals make the same lwIP-implies-IPv6 assumption. Only
  the symbol that broke the build was chased.
* Whether the ESP32 (also lwIP) arm hits this — it had not been reached when this
  was filed.
* An upstream PR. The patch line ships; upstream contribution is a separate line
  on its own schedule (CLAUDE.md's vendored-fork rule).
