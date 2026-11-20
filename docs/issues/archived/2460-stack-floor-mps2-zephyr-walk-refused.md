---
id: 2460
title: "The stack-floor walk is refused on mps2-an385-zephyr images: z_malloc, z_free and z_sleep_us have no frame, because the zpico-sys frame rows are stated for thumbv7em only"
status: resolved
type: limitation
severity: low
area: [memory, zephyr, build]
found: 2026-11-12
resolved: 2026-11-21
resolved_in: "fix(stack-floor): toolchain builtins take frames from their own objects (phase-557 W9b)"
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

## 2026-11-21: the five `z_*` frames come from the compiler (phase-557 W9a)

The Zephyr cargo lanes now compile cc-rs C with `-fstack-usage` whenever the stack
floor is on, so `platform_aliases.c`'s frames are read per build and the five hand
rows are retired. Rebuilt `cpp-port-minimal-publisher` on mps2-an385 from the
branch: no `z_*` function is in the refusal any more. What is left is

```
__aeabi_dadd, __aeabi_dsub, __aeabi_l2d, __aeabi_memclr, __aeabi_memclr4,
__aeabi_memclr8, __aeabi_memcpy, __aeabi_memcpy4, __aeabi_memcpy8, __aeabi_memmove,
__aeabi_memmove4, __aeabi_memmove8, __aeabi_memset, memcmp, memcpy, memmove, memset,
nros_cpp_publish_std_msgs_msg_string, strlen
```

The first three are libgcc's hand-written float routines, which carry no CFI. The rest
come from the C++ FFI image staticlib, which the Zephyr road builds with no
`-Csave-temps`, so neither its own functions nor the `compiler_builtins` that
`-Zbuild-std` compiles into it have bitcode to read. Both are phase-557 W9b, which
closes this issue.

## Resolution (2026-11-21, phase-557 W9a and W9b)

Every function in the refusal now has a frame from something the build or the toolchain
measured, and none is typed by hand:

- **`z_*` (W9a).** cc-rs C is compiled with `-fstack-usage` on both Zephyr cargo lanes, so
  `platform_aliases.c`'s frames are this build's; the five hand rows are gone.
- **libgcc's hand-written routines (W9b).** `__aeabi_dadd`, `__aeabi_dsub`, `__aeabi_l2d` and
  the rest carry no CFI. `just zephyr stack-builtins <archive>` measures every function of a
  toolchain archive from its own objects (Thumb prologue decoding, plus the deepest chain of
  calls inside the archive) into `config/stack-builtins.toml`, keyed by the archive's content
  hash. The walk reads the rows of exactly the archives its link map names. An archive with no
  row is a refusal naming it and the recipe, never a near key's numbers.
- **The C++ FFI image (W9b).** Zephyr's `_nros_zephyr_cpp_ffi_image` lost the stack-floor
  cargo rows (`nros_resolve_board_facts()` re-reads the board facts alone), so the image
  staticlib had no bitcode and `nros_cpp_publish_*` no frame. It now adds them, as the generic
  generator does.
- **`compiler_builtins`' `#[naked]` routines (W9b).** `-Zbuild-std` compiles them into the
  image's staticlib, and `llc --stack-sizes` states no frame for a naked function. The walk
  reads them off the linked objects with the same decoder, and only for what the link map
  PLACED from a `compiler_builtins` member: the staticlib also carries weakened copies of
  libgcc's routines (issue 2092), and those are not the ones linked.

Measured on `cpp-port-minimal-publisher` (mps2-an385, thumbv7m) rebuilt from the branch: the
walk finishes and prints the UNMEASURED floor, 170,284 B walked + margin = 187,320 B. That
walk is issue 2658's over-approximation, and the image has no observed row, so it is reported
and never enforced, as ruling (A) says.

