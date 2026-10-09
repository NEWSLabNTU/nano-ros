---
id: 1765
title: "The `__NROS_SIZE_*` size-probe markers are linked into C/C++ images, so a carved parameter store costs its size twice: once in `.bss`, once in `.text`"
status: open
type: bug
area: [build, cpp, memory]
severity: medium
found: 2026-10-09
related: [1706, 0023, 0464, phase-382]
---

## What

`packages/api/nros/src/sizes.rs`'s `export_size!` emits, per probed type,

```rust
#[cfg_attr(feature = "ffi-size-markers", used)]
#[unsafe(no_mangle)]
pub static __NROS_SIZE_<NAME>: [u8; <NAME>] = [0u8; <NAME>];
```

an array whose SYMBOL SIZE is the number the `nros-c` / `nros-cpp` build
scripts read back (`nros_sizes_build::extract_sizes`). It is a build-time
probe. It is also a `#[no_mangle]` immutable static in the shipped staticlib,
so it reaches the final link and lands in read-only data, which the board
linker scripts place in `.text`.

Until phase-382 W3' that cost the probe's own number in flash -- about 25 KB
for `EXECUTOR_SIZE`, since nobody looked. W3' made `EXECUTOR_SIZE` include the
carved parameter store, so an image that carves a store now carries a second,
all-zero copy of the store's size in `.text`.

## Measured (2026-10-09, issue 1706's AN536 runs)

`workspace-cpp-mps3-an536-freertos` (Cyclone C++ entry, armv7-R), the bringup
declaring `param_services`:

| build | `NROS_EXECUTOR_SIZE` | `__NROS_SIZE_EXECUTOR_SIZE` (`T`) | text | bss |
| --- | --- | --- | --- | --- |
| store on the heap (main) | 24,856 | 24,856 B | 1,263,848 | 1,187,288 |
| store carved (issue 1706) | 305,688 | **305,688 B** | 1,544,736 | 1,468,120 |

`.bss` grows by the store (280,832 B), which is the intended move out of the
heap. `.text` grows by 280,888 B as well, and `arm-none-eabi-nm -S` names the
marker as the symbol that grew. On a flash part that is a 280 KB flash cost
for nothing.

## Direction

The marker must not survive into an image. The `__NROS_SIZE_FN_<NAME>`
fn-pointer marker beside it already encodes the size in its mangled name at
no storage cost (phase-77.25, for fat LTO). Options:

- read only the fn marker and stop emitting the array, once every consumer of
  the legacy path is gone (issue 0464 lists the fallbacks);
- or give the array its own section and discard it at link (`/DISCARD/`), which
  needs every board linker script to agree.

Either way, check every probe in `sizes.rs`, not only `EXECUTOR_SIZE`: the
`RAW_*` probes are sized by the subscription buffer and are the next largest.
Acceptance: `__NROS_SIZE_` absent from `nm` of a linked C/C++ image, and the
probe still reading the right numbers.
