---
id: 2460
title: "The stack-floor walk is refused on mps2-an385-zephyr images: z_malloc, z_free and z_sleep_us have no frame, because the zpico-sys frame rows are stated for thumbv7em only"
status: open
type: limitation
severity: low
area: [memory, zephyr, build]
found: 2026-11-12
related: [issue-2400, issue-2402, phase-557]
---

## What

Issue 2400 W4 runs the stack-floor CHECK on every Zephyr image off `ARCH_POSIX`. On the four
`mps2-an385-zephyr` template leaves (`ament-workspace-port`, `cpp-port-minimal-publisher`,
`rclcpp-compat-smoke`, `topic-state-monitor-port`, all C++ on thumbv7m) the walk is refused:

```
nros-stack-floor: REFUSED
root `…create_publisher_trampoline…`:
  - functions reached with no frame in any input (a missing frame would count as 0): z_free, z_malloc, z_sleep_us
```

Those images are UNMEASURED, so ruling (A) holds and the build only warns (`the walk was refused
for an UNMEASURED image; not enforced`). The cost is that they get **no floor at all**, not even
the static walk + margin an unmeasured image normally reports.

## Why

`zpico-sys`'s `platform_aliases.c` is built by its cc build script, which gets no
`-fstack-usage` under Zephyr. So its five functions carry hand-read frames in
`config/stack-floors.toml` (`[[policy]] kind = "frame"`: `z_malloc`, `z_free`, `z_random_fill`,
`z_sleep_us`, `z_sleep_ms`). Every one of those rows says `target = "thumbv7em-none-eabi"`:
they were read off the nucleo-f767zi (Cortex-M7) prologues. mps2-an385 is
`thumbv7m-none-eabi`, so no row applies there.

## Fix directions

- Preferred: give the `zpico-sys` cc build `-fstack-usage` (and `-fcallgraph-info=su`) when the
  stack floor is on, so the frames are measured per target and the hand rows retire.
- Or: read the same five prologues on thumbv7m and add rows for that target.

Acceptance: `just check template-copy-out` on the four leaves prints an UNMEASURED floor
(walk + margin) instead of a refusal.

## Measured 2026-11-18: the refusal is wider than three functions, and hand rows would be wrong

Re-built `cpp-port-minimal-publisher/zephyr` (mps2-an385, thumbv7m) from main at
5370be0bb9, copied out of the tree. The walk is still refused for root `main`, and
the list of functions with no frame is longer than the one above:

```
functions reached with no frame in any input (a missing frame would count as 0):
  __aeabi_memclr, __aeabi_memclr4, __aeabi_memclr8, __aeabi_memcpy, __aeabi_memcpy4,
  __aeabi_memcpy8, __aeabi_memmove, __aeabi_memmove4, __aeabi_memmove8, memcmp, memcpy,
  memmove, memset, nros_cpp_publish_std_msgs_msg_string, rclc_log_emit_fmt_at, strlen,
  z_free, z_malloc, z_random_fill, z_sleep_ms ...
```

The walk also notes that the link used a `compiler_builtins` that no installed
`thumbv7m-none-eabi` toolchain holds, which is where the `__aeabi_*` and `mem*`
symbols come from.

So adding rows for the five `platform_aliases.c` functions would not clear the
refusal. The acceptance above needs the builtins and the two C/C++ functions as well.

The five prologues on thumbv7m, read with the pinned `arm-none-eabi-objdump` from
`zpico-sys`'s `platform_aliases.o` in that build:

| function | prologue | frame |
| --- | --- | --- |
| `z_malloc` | `push {r7}` ... `pop {r7}`; `b.w nros_platform_alloc` | 4 |
| `z_free` | `push {r7}` ... `pop {r7}`, tail call | 4 |
| `z_random_fill` | `push {r7}` ... `pop {r7}`, tail call | 4 |
| `z_sleep_us` | `push {r7, lr}`; `bl nros_platform_sleep_us` | 8 |
| `z_sleep_ms` | `push {r7, lr}`; `bl nros_platform_sleep_ms` | 8 |

These DIFFER from the thumbv7em rows in `config/stack-floors.toml`: there, `z_malloc`,
`z_free` and `z_random_fill` are 0 (a bare `b.w`). Here this build keeps the frame
pointer (`r7`), so the same source costs 4 bytes. A hand-read frame is a fact about
one set of compile flags, not about a target. A `target =` row would be right for
one build and silently low for another, which is the failure the walk exists to
prevent.

This makes the first fix direction the only sound one: give the `zpico-sys` cc build
`-fstack-usage` (and `-fcallgraph-info=su`) whenever the stack floor is on, and
retire the five hand rows. The builtins need their own answer: either frames from
the linked image's `.debug_frame`, as the walk already does for 178 functions, or a
`compiler_builtins` from the provisioned toolchain.
