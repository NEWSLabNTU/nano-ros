---
id: 1765
title: "The `__NROS_SIZE_*` size-probe markers are linked into C/C++ images, so a carved parameter store costs its size twice: once in `.bss`, once in `.text`"
status: resolved
type: bug
area: [build, cpp, memory]
severity: medium
found: 2026-10-09
related: [1706, 0023, 0464, 0593, 0665, phase-382]
resolved_in: "fix(#1765): the size-probe markers are probe-only and never reach a linked image"
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

## Resolution (2026-10-10): the markers are probe-only

Neither direction above was taken. Both keep the markers in the shipped
staticlib and remove them later, either from the reader or at the link. The
fix keeps them out of every build that is linked.

### Why they were linked

`#[cfg_attr(feature = "ffi-size-markers", used)]` only decided whether the
markers were `#[used]`. The statics themselves were always compiled, and a
`#[no_mangle]` static is EXPORTED from a staticlib whether or not it is
`#[used]`. The feature was also ON in every C/C++ image, because phase-361 W3
requested it on `nros-c`'s and `nros-cpp`'s own `nros` dependency. That was the
only way the probe got it: probe forwarding (issue 0665) carries the consumer's
`nros` features into the nested build. So the markers reached the final link
(as `T` symbols in `.text`), along with the `__nros_size_<NAME>::<N>`
monomorphisations the fn-pointer markers point at.

### What changed

- `packages/api/nros/src/sizes.rs`: all three marker items (the array static,
  the generic fn and the fn-pointer static) are
  `#[cfg(feature = "ffi-size-markers")]`. The `pub const`s are unchanged.
- `nros-sizes-build`: `find_dep_rlib_with_probe_features` adds the features
  that only the probe's build may turn on. They join the probe-dir key, so a
  marked rlib never shares a directory with an unmarked one. The nested cargo
  is marked `NROS_SIZES_PROBE_BUILD=1` (`PROBE_BUILD_ENV`).
- `nros-build-helpers::shared::probe_rlib` requests `ffi-size-markers`. It is
  now the only requester. `nros-c` and `nros-cpp` no longer name the feature
  on their dependency.
- `nros/build.rs` panics if `ffi-size-markers` is on and
  `NROS_SIZES_PROBE_BUILD` is not `1`. A dep-site, a `--features`, a cmake
  `FEATURES` list or a whole-workspace unification that turns it on fails the
  build instead of shipping the bytes. Measured: `cargo check -p nros
  --features rmw-cffi,ffi-size-markers` panics with the issue number, and the
  same command with `NROS_SIZES_PROBE_BUILD=1` compiles.

### Measured (2026-10-10, incremental rebuilds, no build dir wiped)

| image | road | text before | text after | delta | markers before |
| --- | --- | --- | --- | --- | --- |
| `workspace-cpp-mps3-an536-freertos` (committed bringup) | cmake, Cyclone C++ | 1,174,752 | 1,127,928 | -46,824 | 54 symbols, 46,900 B (`EXECUTOR_SIZE` 24,976) |
| same, bringup `features = ["param_services"]` (local edit, not committed) | cmake, Cyclone C++ | 1,546,552 | 1,218,824 | **-327,728** | 54 symbols, 327,784 B (`EXECUTOR_SIZE` **305,832**) |
| `examples/mps2-an385-freertos/c/talker` | cmake, zenoh C | 587,256 | 476,008 | **-111,248** (19 %) | 51 symbols, 111,252 B (`EXECUTOR_SIZE` 89,280) |

After the fix, `nm` finds 0 marker symbols in all three images. `.bss` and
`.data` are byte-identical before and after in every row (for example
34,367,336 / 14,140 for the `param_services` image), so nothing moved: the
`.text` delta is exactly the markers.

**The probe still reads the right numbers.** Every `#define` in the generated
`nros_cpp_config_generated.h` (AN536, 19 lines, including
`NROS_EXECUTOR_SIZE 305832`) and `nros_config_generated.h` (C talker, 27 lines)
is identical before and after, apart from the variant hash. The sizes come from
the probe's own marked rlib, as before.

### Gate: `check-size-markers-unlinked` (two recipes, one script)

- `just check size-markers-unlinked` (fast line, buildless, 0.4 s). It
  enforces three source rules:
  - every marker item is `cfg`'d on its probe-only feature
    (`ffi-size-markers`, plus `layout-size-markers` for nros-node's
    `__NROS_LU_SZ_*`);
  - no tracked file enables either feature, except the probe;
  - the refusal is wired, with the two `PROBE_BUILD_ENV` spellings agreeing.
- `just check size-markers-image` (`--built`): `nm`s every final linked image
  under the fixture roots derived from the manifest. That covers cmake leaves
  and workspaces, cargo groups and the Corrosion dir, west leaves'
  `zephyr.{exe,elf}`, and the NuttX kernel. The scan is scoped to the run's
  lane (`NROS_TEST_COORDS`) and prints the image count per road. An empty
  scan is NOT VERIFIED, never OK. It is a step of `just ci` tier 1 (`tests`),
  `matrix`, `matrix-nightly` and `full`, after the fixture build.

**Reach.** Every road compiles `nros` through cargo, using features that a
tracked file names (manifest, recipe, cmake, `Make.defs`). Rules B and C
therefore bind on cargo, cmake/Corrosion, west and the NuttX `Make.defs` road
by construction. The image scan is the linker-side witness for whichever roads
a lane built.

**Mutation checks (run, not reasoned):**

| mutation | result |
| --- | --- |
| `sizes.rs` array static back to `cfg_attr(.., used)` + `nros-c` dep requests the feature again | fast gate FAILS, naming `sizes.rs:71` and `nros-c/Cargo.toml:379` |
| the same dep-site request, with `build.rs`'s refusal disabled, then the 6 FreeRTOS C zenoh images rebuilt | `--built` FAILS: 306 marker symbols across the 6 images. Restored and rebuilt: OK, 15 images (cmake=14, nuttx-make=1) |
| the dep-site request alone | `nros/build.rs` refuses the build |

The self-test runs on every invocation. It covers each rule's negative and
positive cases, comments that are not code, and a crafted ELF32 with a real
symbol table: a defined marker symbol is found, while a clean image and an
image that only QUOTES the name in a string are not.

### Not measured

No Zephyr (west) or NuttX image was rebuilt here. Both compile `nros-c` /
`nros-cpp` through the same manifests changed above, so the markers cannot be
compiled there either (reasoned, not measured). The image scan reports zero
images on the `west` road for any lane that built none.
