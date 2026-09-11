---
id: 1324
title: "A native_sim XRCE image's Rust `#[global_allocator]` is the HOST glibc heap,
  so the nine XRCE cells cannot fail an allocation the zenoh cells would"
status: open
type: bug
area: [zephyr, testing, embedded]
severity: low
found: 2026-09-11
related: [1189, 1010, 0163, 0594, phase-391, phase-448]
---

## What

On `native_sim`, a Zephyr XRCE image links `CONFIG_EXTERNAL_LIBC=y` and a Zephyr
zenoh image links `CONFIG_PICOLIBC=y`. Issue 1189 measured that and explains why
(`CONFIG_POSIX_API` `select`s `NATIVE_LIBC_INCOMPATIBLE`, which flips Zephyr's
`choice LIBC_IMPLEMENTATION` off its `NATIVE_BUILD` arm); it concludes,
correctly, that neither overlay should pin a libc. This is the consequence 1189
deliberately did not fold into itself.

A Zephyr **Rust** leaf's `#[global_allocator]` is zephyr-lang-rust's
`ZephyrAllocator` (`modules/lang/rust/zephyr/src/alloc_impl.rs`), which is a
thin wrapper over libc `malloc`/`free`. So:

| image | Rust global allocator resolves to | bounded by |
| --- | --- | --- |
| zenoh / native_sim | picolibc `malloc` | `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` (a `.bss` arena) |
| xrce / native_sim | **host glibc `malloc`** | **nothing** |

Note this is a SECOND heap either way: `nros_platform_alloc` does not go through
it. Since phase-391 W3 the nano-ros funnel is `nros-platform`'s rlsf arena
(`NROS_ZEPHYR_HEAP_SIZE`), a `.bss` static, identical under either libc — which
is why this has nothing to do with issue 1010 and why 1010's fix is unaffected.

## Why it matters

The nine `ZephyrNativeSim × Xrce` cells exist to be the affordable proxy for the
embedded XRCE surface. An over-allocating change to the Rust side of an XRCE
image cannot fail them: the host heap absorbs it. The same change in a zenoh
image hits a fixed arena and fails. So the XRCE cells are systematically weaker
than their zenoh twins, and nothing says so — the two look like the same cell
with a different `rmw` column.

It is also a RAM-accounting hole: `just mem-report` reads symbols, and an
allocation served by the host heap has none.

## Not yet measured

**How many bytes a Zephyr Rust XRCE image actually allocates through the Rust
global allocator.** If it is small, the gap is theoretical and the cheapest fix
is a note; if it is not, the cells are measuring less than they appear to. This
is the measurement to take first, and the obvious instrument is to select
picolibc on one XRCE leaf with the DEFAULT 16 KiB arena and see whether it still
runs.

## Candidate fixes, in the order they should be considered

1. **Measure first** (above).
2. Give the Zephyr Rust leaves `nros-platform/global-allocator`, so the ONE
   arena rule of RFC-0034 D6 actually holds on Zephyr and both RMWs are bounded
   by `NROS_ZEPHYR_HEAP_SIZE`. This is the fix that makes the libc irrelevant
   rather than making the two overlays match, and it removes a second heap from
   every Zephyr Rust image, not just the XRCE ones. It needs care: a duplicate
   `#[global_allocator]` against zephyr-lang-rust's is a link error, so
   `CONFIG_RUST_ALLOC` and this feature are mutually exclusive.
3. Pin `CONFIG_PICOLIBC=y` on the XRCE overlays. Rejected in 1189 with reasons;
   listed here only so the next reader does not re-derive it as new.

## Reproduce

```
just zephyr build-one rust/listener xrce
grep -E 'EXTERNAL_LIBC|PICOLIBC=|COMMON_LIBC_MALLOC_ARENA_SIZE' <build>/zephyr/.config
just zephyr build-one rust/listener zenoh
grep -E 'EXTERNAL_LIBC|PICOLIBC=|COMMON_LIBC_MALLOC_ARENA_SIZE' <build>/zephyr/.config
```
