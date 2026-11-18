---
id: 2692
title: "Some cc-rs C builds take their CPU and FPU from cc-rs's compiler defaults, not the image's arch profile"
status: resolved
resolved_in: "the change that archives this issue"
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

## Fix (2026-11-18)

One carrier, no per-site helper (manager ruling, shape A). This follows the
road phase-550 W3b took for NuttX.

- `nros_board_common::arch_flags::arch_cflags_rows` turns the image's
  `[arch.*]` profile into `CFLAGS_<triple>` / `CXXFLAGS_<triple>`. cc-rs
  reads these for every build of that triple, so every `cc::Build` in the
  image gets the profile, today's and any added later.
- **Precedence.** The row is the profile's flags, then whichever variable
  cc-rs would otherwise have read (`<KIND>_<triple>`, its `-` spelling,
  `TARGET_<KIND>`, `<KIND>`), merged. It is written `force = true`, so an
  exported variable can neither silently replace the profile nor be
  silently dropped. A user flag comes after the profile, so for gcc's
  last-one-wins options it still wins. When a user value was merged, the
  build prints one line naming the variable, its value and the result. It
  prints nothing in the common case.
- **Roads.** All three go through `cargo_config::arch_cflags_env_for`:
  - cargo: `nros build`'s `nros-cargo.toml`, and the leaf settings;
  - cmake: the image-env file, both from `nros build`'s configure and from
    `leaf-system --image-env-out`;
  - west: the same file, written beside the Kconfig fragment.
  - NuttX's make road passes through it too, but answers nothing:
    `[arch.cortex-a7]` sets `cpu_flags_from_kernel_export`, because there
    the export's `TARGET_CFLAGS` is the source (W3b), and a `CFLAGS_<triple>`
    row would shadow it.
- **Tests.**
  - `arch_flags`: the merge order, a blank value, and the shipped profiles.
  - `cargo_config`: the forced row.
  - `leaf_system`: the image-env file, and both leaf roads for
    `mps3-an536-freertos`, with `--board` taking none.
- **Acceptance.** `just check out-of-tree-board-images` now reads
  `readelf -A` over every member of every ARM archive in the three
  `fixture-r5f-freertos` image builds. Each member must say R-profile,
  VFPv3-D16 and VFP registers. The C build runs with a user `CFLAGS`
  exported and must report the merge. Its selftest refuses an archive that
  holds one M-profile object.
- **What the acceptance measured, beyond the fix.** Its first run failed on
  one member per road: FreeRTOS's `portASM.S` (profile R, VFPv3-D16, no
  `Tag_ABI_VFP_args`). That object is built correctly. GNU as writes no
  float-ABI tag for assembly even with `-mfloat-abi=hard` (measured, gcc
  13.2), and a member that states no calling convention makes no ABI claim.
  So the float ABI is asked only of a member that states one (any
  `Tag_ABI_*` attribute), which every compiled object does. Profile and FPU
  are still asked of every member. The selftest gained both halves: an
  archive with a hard-float assembly member passes, and one with a softfp C
  member is refused. Mutation-checked: the old rule refuses the assembly
  archive.

Kept (retired by [issue 2709](../2709-duplicate-arch-cflags-carriers.md)), now redundant where an image is configured:
`nros_arm_r_fpu_cflags_env` / `nros_riscv64_rustflags_env`'s CFLAGS rows (the
cmake corrosion env), and `nros-board-threadx-qemu-riscv64`'s
`CFLAGS_riscv64gc_unknown_none_elf`. Both state the profile's own value, and
where both apply the forced carrier row wins. Retiring them needs a build of
every cmake/RISC-V road that no lane has measured yet.

Follow-up: [issue 2709](../2709-duplicate-arch-cflags-carriers.md) retires the two
kept duplicate carriers once a lane measures them.
