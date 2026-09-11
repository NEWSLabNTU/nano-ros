---
id: 1189
title: "The six Zephyr XRCE Rust leaves set `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`,
  and their images do not select picolibc — so the knob never reaches the
  resolved `.config` and the comment above it describes another image"
status: resolved
type: bug
area: [embedded, build]
severity: low
found: 2026-09-06
resolved_in: phase-448 W1 — dead line removed, libc divergence measured and explained
related: [1145, 1171, 0876, 0163, 1010, phase-448]
---

## What

Every `examples/zephyr/rust/*/prj-xrce.conf` carries:

```
# Issue 0163 — the Zephyr Rust global allocator is picolibc malloc, so the
# executor's ~75 KB leaked default backing (phase-271) and the in-image
# backend's buffers come from COMMON_LIBC_MALLOC_ARENA_SIZE (default 16 KB),
# NOT the kernel heap.
CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=1048576
```

The XRCE images do not use picolibc, so that symbol is never selected and the
line does nothing.

## Measured

Resolved `.config` of four Zephyr Rust images built from the same tree:

| image | `CONFIG_PICOLIBC=y` | `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` |
| --- | :-: | --- |
| `listener` / xrce / native_sim | **no** | **absent** |
| `talker` / cyclonedds / native_sim | yes | 16688888 |
| `talker` / zenoh / native_sim | yes | 961320 |
| `action-server` / zenoh / mps2_an385 | yes | 960248 |

The XRCE image reports `CONFIG_ZEPHYR_PICOLIBC_MODULE=y` and
`CONFIG_FULL_LIBC_SUPPORTED=y` with `# CONFIG_PICOLIBC_MODULE is not set` — it
links the full libc, whose allocator is not the `COMMON_LIBC` arena.

## Why it matters

Three ways, in increasing order:

1. **The comment is wrong about that image**, and it is the comment a reader
   consults when sizing an XRCE board. It names picolibc as "the Zephyr Rust
   global allocator" without qualification.
2. **A 1 MiB number that looks load-bearing is not.** Anyone budgeting RAM for
   an XRCE image from this file starts from a figure the image never had.
3. **It nearly produced a false claim.** Issue 1145's sweep paired the arena
   against `EXECUTOR_BACKING` across all 18 Zephyr Rust confs, and the pairing
   arithmetic is checked textually by `check-executor-backing-arena-pairing` —
   which cannot know whether the symbol reaches the image. The six XRCE confs
   passed the gate while pairing a knob that does not exist. Caught by building
   one and reading its `.config`; the six were reverted and only the twelve
   picolibc confs paired.

## What is NOT wrong

`EXECUTOR_BACKING` **is** in the XRCE image — measured at 88,328 B, same as its
siblings. phase-392 W6 applies there like everywhere else. What is absent is the
fixed arena those bytes used to come out of, so on XRCE there is nothing to
lower and no double reservation to remove. The right answer for these six confs
is therefore "state nothing", which is what they now do.

## Fix direction

Delete the dead line and correct the comment, OR — if an XRCE image *should* be
on picolibc like its siblings, which is a real question nobody has asked here —
select it and keep the knob. **Do not simply delete without answering that**: a
silent libc difference between the RMW variants of the same example is a bigger
finding than a dead config line, and it may explain other divergence between
the XRCE and zenoh Zephyr lanes.

Same family as issue 0876 (a leaf Kconfig value that changes nothing because a
later fragment sets it), one mechanism over: here nothing overrides the value,
the symbol is simply not part of the image's configuration.

## Reproduce

```
west build -b native_sim/native/64 -d <dir> examples/zephyr/rust/listener -- \
  -DCONF_FILE="prj.conf;prj-xrce.conf;cmake/zephyr/native-sim-line-3.7.conf" …
grep -E 'PICOLIBC=|COMMON_LIBC_MALLOC_ARENA_SIZE' <dir>/zephyr/.config
```

## Resolution (phase-448 W1, 2026-09-11)

**The question this issue refused to answer without evidence — *should* an XRCE
image be on picolibc like its zenoh siblings? — is answered NO.** The dead line
and its comment are gone from all six overlays; no overlay pins a libc. What
follows is the measurement and the reasoning, because the answer is only worth
as much as its reason.

### Measured

Built `examples/zephyr/rust/listener` and `examples/zephyr/cpp/listener` with
`prj-xrce.conf` on `native_sim/native/64` and read the resolved `.config`:

| image | `CONFIG_EXTERNAL_LIBC` | `CONFIG_PICOLIBC` | `CONFIG_PICOLIBC_SUPPORTED` | `COMMON_LIBC_MALLOC_ARENA_SIZE` |
| --- | :-: | :-: | :-: | --- |
| rust / xrce / native_sim | **y** | absent | y | **absent** |
| cpp / xrce / native_sim | **y** | absent | y | **absent** |
| rust / zenoh / native_sim (this issue's table) | no | y | y | 961320 |

So the divergence is not a property of the *Rust* leaves, as the title has it —
it is the whole XRCE lane, C and C++ included. Those two languages never set the
arena knob, which is the only reason nobody noticed.

### Why the two RMWs differ

Neither overlay chooses a libc. Zephyr 3.7 `lib/libc/Kconfig`:

```
choice LIBC_IMPLEMENTATION
	default EXTERNAL_LIBC if NATIVE_BUILD && !(NATIVE_LIBRARY && NATIVE_LIBC_INCOMPATIBLE)
	default PICOLIBC
	...
```

and `lib/posix/options/Kconfig.profile`:

```
config POSIX_API
	select NATIVE_LIBC_INCOMPATIBLE
```

`prj-zenoh.conf` sets `CONFIG_POSIX_API=y` because zenoh-pico needs pthreads
(the same overlay raises `MAX_PTHREAD_MUTEX_COUNT`), which selects
`NATIVE_LIBC_INCOMPATIBLE`, which falsifies the first arm, so a zenoh image
falls through to `default PICOLIBC`. The XRCE backend needs no POSIX option —
its Zephyr transport is `transport_nros_udp.c` through the platform ABI, and
`UCLIENT_PLATFORM_POSIX` is bound to the `posix` condition, which a Zephyr build
does not answer — so the XRCE image keeps the first arm and links the host libc.

### Why "state nothing" is the right answer and `CONFIG_PICOLIBC=y` is not

1. **The whole `EXTERNAL_LIBC` arm is `NATIVE_BUILD`-only.** On any real board
   the choice falls through to `default PICOLIBC` for both RMWs already. All 18
   Zephyr XRCE rows in `examples/fixtures.toml` are `native_sim/native/64`, so
   the difference exists today only in the host simulator, and pinning
   `CONFIG_PICOLIBC=y` would override a Zephyr *board* decision in order to make
   two overlays look alike.
2. **The 1 MiB was never measured for an XRCE image.** It was copied from
   `prj-zenoh.conf`. Keeping it live would carry an unmeasured number onto the
   first real board an XRCE example is built for — worse than carrying no
   number, because it reads as a decision.
3. **It does not explain issue 1010, which is the divergence people actually
   hit.** The allocation that killed every Zephyr XRCE image is
   `nros_platform_alloc` → `nros-platform`'s rlsf arena
   (`NROS_ZEPHYR_HEAP_SIZE`, a `.bss` static since phase-391 W3), which is
   byte-identical under either libc. Checked before believing the libc was
   implicated.

### What is left, and is NOT this issue

Under `EXTERNAL_LIBC` the Rust `#[global_allocator]` on a Zephyr Rust leaf —
zephyr-lang-rust's `ZephyrAllocator`, which calls libc `malloc` — resolves to
the **host glibc heap**: unbounded, unaccounted, and not what the same image
gets on a board, where the zenoh cells are bounded by picolibc's arena. That is
a test-fidelity gap in the native_sim XRCE cells, not a wrong line in a conf,
and it is filed separately as **issue 1324**.
