---
id: 2203
title: "zephyr-lang-rust hands GCC's builtin include dir to bindgen, so no Rust Cortex-M image builds on Zephyr 4.4"
status: resolved
resolved_in: "2026-11-01"
type: bug
area: [zephyr, build]
severity: high
found: 2026-11-01
related: [2187, 0432, 2185]
---

## What was measured

A Rust image for `xiao_rp2350/rp2350a/m33` (RP2350, Cortex-M33,
`thumbv8m.main-none-eabi`) on Zephyr v4.4.0 with Arm GNU 13.2 (`gnuarmemb`)
fails in `zephyr-sys`'s build script:

```
.../arm-none-eabi/13.2.1/include/arm_acle.h:41:3: error: argument to '__builtin_arm_cdp' must be a constant integer
... (18 errors)
thread 'main' panicked at .../zephyr-sys/build.rs:118:10:
Unable to generate bindings: ClangDiagnostic(...)
```

The only way through was an environment variable:
`BINDGEN_EXTRA_CLANG_ARGS="-D_GCC_ARM_ACLE_H -include <clang>/arm_acle.h"`.

It is not specific to that board, that core or that toolchain. Measured with
zephyr-lang-rust's own `samples/hello_world`, stock module, Zephyr 4.4.0:

| board | toolchain | stock | with the fix |
|---|---|---|---|
| `mps2/an385` (Cortex-M3) | Arm GNU 13.2 | 18 errors | builds |
| `mps2/an385` (Cortex-M3) | Zephyr SDK 1.0.1 (GCC 14.3) | 18 errors | builds |
| `xiao_rp2350/rp2350a/m33` | Arm GNU 13.2 | 18 errors (talker) | talker builds and links |

The header alone fails the same way under libclang 14 and libclang 18, for
`thumbv7m`, `thumbv7em` and `thumbv8m.main` (`thumbv6m` and `thumbv8m.base`
pass, because GCC guards the coprocessor intrinsics with `__thumb2__`).

## Cause

`rust_cargo_application()` passes every entry of `zephyr_interface`'s
`INTERFACE_INCLUDE_DIRECTORIES` to `zephyr-sys/build.rs` as `INCLUDE_DIRS`, and
build.rs makes each one a `-I` for bindgen. Zephyr builds with `-nostdinc`, so it
puts the C compiler's own header directory on that target
(`$<TARGET_PROPERTY:compiler,nostdinc_include>`, which is
`gcc -print-file-name=include`). bindgen is clang, and a `-I` is searched
before clang's resource directory, so every compiler-private header clang asks
for resolves to GCC's copy.

CMSIS_6 makes that fatal. Its m-profile headers include `<arm_acle.h>`
unconditionally (`cmsis_clang_m.h`, which is the branch clang takes). GCC's
`arm_acle.h` defines `__arm_cdp` and friends as inline functions that pass a
parameter to `__builtin_arm_cdp`, which clang requires to be an integer
constant. The parse fails before bindgen generates anything.

The 3.7 line uses CMSIS 5, whose headers never include `arm_acle.h`, so the
defect was latent there and every 3.7 Rust Cortex-M image looked fine.

## Fix

`scripts/zephyr/zephyr-lang-rust-bindgen-include-patch.sh` patches
zephyr-lang-rust's `get_include_dirs()` to remove the compiler's builtin
include directory from the list, in both spellings: the generator expression
(how 3.7 and 4.4 store it) and the evaluated `nostdinc_include` paths. Clang
then takes `stddef.h`, `stdarg.h`, `arm_acle.h` and the rest from its own
resource directory, which is what a Zephyr build with the clang toolchain does
anyway. Nothing else reads `INCLUDE_DIRS`, and the C build is unaffected
because its include path comes from the target, not from this copy.

It is applied beside the gpio patch (issue 0432) everywhere that one is:
`patches/3.7.sh`, `patches/4.4.sh` (which `nros setup board` and
`just zephyr setup` run), `just/zephyr-ci.just` and the FVP board-import recipe.
`zephyr/patches.yml` gains `zephyr-lang-rust-bindgen-include.patch` for
downstream 4.x workspaces using `west patch`. It is a generic zephyr-lang-rust
bug and upstreamable.

Applied to 3.7 as well, so bindgen sees the same include path on both lines.
On 3.7 it changes nothing: `mps2/an385` `hello_world` with Zephyr SDK 0.16.8
produces byte-identical `bindings.rs` with and without the patch. On the 4.4
talker the only difference against the workaround build is
`va_list = __gnuc_va_list` becoming `va_list = u32` (clang's `stdarg.h`
instead of GCC's), which is the same type.

## Measured after

- `xiao_rp2350/rp2350a/m33` Rust talker (zenoh over UART), Zephyr 4.4.0,
  Arm GNU 13.2, `BINDGEN_EXTRA_CLANG_ARGS` unset: builds and links,
  FLASH 382320 B, RAM 319312 B.
- `mps2/an385` `hello_world`, Zephyr 4.4.0: builds with Arm GNU 13.2 and with
  Zephyr SDK 1.0.1's GCC 14.3.
- `mps2/an385` `hello_world`, Zephyr 3.7: builds, bindings unchanged.
- The script is idempotent (a second run reports "already applied"), skips a
  workspace without a rust module, and the `.patch` applies to a clean
  `a763400` checkout and to a workspace already carrying the other patches.

## Not resolved here

- **No gate.** The failure only shows in a Zephyr 4.4 Rust Cortex-M build, which
  needs the 4.4 workspace and an Arm toolchain. No merge-gating lane has either.
  The in-tree rows that would catch it are
  `build-cortex-m-rust-talker{,-serial}-zenoh` (`mps2/an385`) on the 4.4 line.
  There is no in-tree Rust Armv8-M Zephyr target. It would add nothing for this
  defect, which hits every Thumb-2 core alike.
- **Issue 2187 says those rows built on 4.4**, which this measurement
  contradicts: the stock module fails for `mps2/an385` with both toolchains
  above and under libclang 14 and 18. 2187 does not record its libclang or
  toolchain, so that is not reconciled here.
- `patches/4.4.sh` does not run `cargo-features-patch.sh`, unlike
  `patches/3.7.sh`. A 4.4 workspace provisioned only through
  `nros setup board` therefore lacks `${NROS_BOARD_FACTS_ENV}` on the cargo
  command line (the class issue 2185 describes for 3.7). That is a separate
  defect and is not changed here.
