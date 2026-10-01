---
id: 1324
title: "A native_sim XRCE image's Rust `#[global_allocator]` is the HOST glibc heap,
  so the nine XRCE cells cannot fail an allocation the zenoh cells would"
status: resolved
type: bug
area: [zephyr, testing, embedded]
severity: low
found: 2026-09-11
related: [1424, 1611, 1613, 1189, 1010, 0163, 0594, phase-391, phase-448]
resolved_in: "branch fix/zephyr-heap-1424-1425-1498-1324"
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

## Resolution

Fixed by candidate 2 (one allocator, one heap), after taking the measurement
candidate 1 asked for first. Branch `fix/zephyr-heap-1424-1425-1498-1324`.

**Measured first** -- `examples/zephyr/rust/talker`, XRCE, `native_sim/native/64`
(Zephyr 3.7), under gdb with breakpoints on the `__rust_alloc` /
`__rust_dealloc` / `__rust_realloc` / `__rust_alloc_zeroed` shims, 10 s against
the SDK `MicroXRCEAgent`:

    RUSTALLOC calls=40 total=28447 live=27118 peak=28391

So the host heap carried **28,391 B** at peak -- more than the nros platform
heap's OWN peak in the same image (17,680 B, read from its boot record). Not
theoretical: these cells were measuring roughly half of the image's heap.

**The fix:**

* `nros-platform[platform-zephyr]` implies `global-allocator`, so every Zephyr
  image's Rust `alloc` goes through `nros_platform_alloc` -- the rlsf arena sized
  by `CONFIG_NROS_ZEPHYR_HEAP_SIZE`, the one the boot record measures.
* `zephyr/CMakeLists.txt` refuses `CONFIG_RUST_ALLOC=y` at configure, naming
  this issue: two `#[global_allocator]`s do not link, and rustc's error names no
  Kconfig symbol. The 13 Rust Zephyr confs (6 examples, 6 workspace boards, the
  FVP board) and the `nros new` entry template drop the line.
* `zephyr::set_logger()`'s full-`CONFIG_LOG` arm exists only under
  `CONFIG_RUST_ALLOC`, so both Zephyr entry macros (`zephyr_component_main!`,
  `nros::main!`'s Zephyr arm) install nros-log's `log` bridge
  (`nros_platform::log::install_log_crate_bridge`, nros-platform-cffi
  `log-compat`) instead -- `log::info!` and `nros_log` now share one funnel.
  Measured: the talker's `Publishing: 'Hello World: N'` lines still print.
* `CONFIG_NROS_ZEPHYR_HEAP_SIZE` defaults to 131072 on a RUST image (Kconfig
  `default 131072 if RUST`). The heap now carries what it did not before, and
  the gate from issue 1424 said so: the after-image of the same talker peaked at
  **46,256 B** (17,680 + 28,391 within rlsf rounding) and was REFUSED at 64 KiB
  (19,792 spare < 24,576). Measured one-heap peaks, 10 s against a live peer:
  rust/talker xrce 46,256; rust/listener zenoh 42,304; rust/action-server zenoh
  58,288 (7,760 spare at 64 KiB); rust/action-client zenoh 52,032 (79,552 spare
  at the new 128 KiB default). The six workspace confs that sized picolibc at
  1 MiB for a ~75 KiB parameter-service allocation state 262144 explicitly.

**Before/after on the same image:** before, `CONFIG_EXTERNAL_LIBC=y`, the
`zephyr::alloc_impl::ZephyrAllocator` symbol present, 28,391 B on the host heap;
after, `# CONFIG_RUST_ALLOC is not set`, 0 `ZephyrAllocator` symbols, one heap
whose peak the boot record reads.

**Not measured:** the 4.4 line's zephyr-lang-rust (only the 3.7 workspace was
built); the workspace Rust images' new 256 KiB value (stated, not measured --
the per-cell heap gate is what will catch it); an mps2-an385 Rust image's RAM
budget with the 128 KiB default. Filed from this: issue 1611 (the
`COMMON_LIBC_MALLOC_ARENA_SIZE=960024` lines and
`check-executor-backing-arena-pairing` still pair a libc arena Rust no longer
uses), issue 1613 (the `log` bridge records every facade line under the
default logger name, so `rustapp:` reads `nros:`).

Sweep: `git grep -n 'CONFIG_RUST_ALLOC\|set_logger()' -- examples packages zephyr`
