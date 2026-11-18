---
id: 2692
title: "Some cc-rs C builds take their CPU and FPU from cc-rs's compiler defaults, not the image's arch profile"
status: open
type: bug
severity: medium
area: [build, boards, toolchain]
related: [phase-575]
found: 2026-11-18
---

**Home: phase-575 W3** (linking nano-ros into an image TI Arm Clang builds).

## What happens

An `[arch.*]` profile (`packages/platform/*/nros-platform.toml`) states an
image's `-mcpu`/`-mfpu`/`-mfloat-abi`. zenoh-pico's C build applies it
(`nros_zpico_build::apply_arch`), and so do the FreeRTOS and ThreadX board
builders. Other cc-rs builds compiled into the same image do not, and get
their CPU from cc-rs's own defaults. Those defaults depend on which compiler
cc-rs is driving:

- **gcc:** cc-rs derives `-march=armv7-r -mfpu=vfpv3-d16` from
  `armv7r-none-eabihf`, so the object comes out right. Measured on
  `fixture-r5f-freertos`: all 3170 archive members are
  `Tag_CPU_arch_profile: Realtime`, `Tag_FP_arch: VFPv3-D16`.
- **A clang-family compiler:** cc-rs passes `--target=<triple>` instead.
  TI Arm Clang rejects `armv7r-none-eabihf` ("unsupported target triple ...
  for compiler vendor 'ti'"), so its board overrides the target, and the
  builds with no profile fell back to tiarmclang's default CPU. Measured on
  the same board with tiarmclang: `variant_symbol.o`,
  `weak_platform_log_stubs.o` and `log_fmt.o` (all nros-c's) came out
  M-profile (`Tag_CPU_arch_profile` 77) with VFPv4-D16, inside an R5F image.
  GNU ld links such a mix without a word. TI's tiarmlnk reported it
  (warnings 16004-D, 16016-D).

So no GCC image is wrong today. What is missing is that an object's CPU is
decided by the image's profile on every road and with every compiler.

## What would fix it

- Every cc-rs build that lands in an image applies the image's arch profile
  through one helper, whatever the compiler. Candidates from
  `git grep -ln 'cc::Build::new()'` with no profile use: nros-c's
  (`nros-build-helpers/src/{c,shared}.rs`), `nros-board-freertos/build.rs`,
  `nros-board-mps2-an385-freertos/build.rs`, `nros-rmw-cyclonedds-sys`,
  `nros-rmw-xrce-cffi`, `rmw/cffi`, `nros-platform-cffi`, and
  `nros-board-common`'s NuttX/ThreadX/CAN builders. Not every candidate is
  cross-compiled; check each one.
- A gate that reads `readelf -A` over every object in a built image and
  requires the profile's `Tag_CPU_arch_profile`, `Tag_FP_arch` and
  `Tag_ABI_VFP_args`. Then the next mismatch fails on GCC too, instead of
  only under a linker strict enough to notice.

Sweep: `git grep -ln 'cc::Build::new()' -- 'packages/**/build.rs' 'packages/**/src/*.rs'`.
