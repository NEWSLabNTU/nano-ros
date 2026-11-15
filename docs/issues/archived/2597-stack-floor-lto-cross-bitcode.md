---
id: 2597
title: "The post-link stack-floor check fails on a fat-LTO cross image: it derives a bitcode name rustc shortened, and runs another toolchain's llc"
status: resolved
type: bug
area: [build, stack-floor]
severity: high
found: 2026-11-15
resolved: 2026-11-15
related: [2400, 2521, phase-557]
---

Reported from a downstream project (GitHub #3922) on 0.6.0-nros10, on two
FreeRTOS images (`host_freertos` on freertos-posix, `s32z2` on
s32z270-freertos), each a mixed C++ + Rust image built with
`nros-minsizerel` (`lto = "fat"`). The link succeeds and the post-link check
(`scripts/nros-stack-floor.py`) then fails. The only way past it was
`NROS_STACK_FLOOR=OFF`, which hides the check, so the project held its gate
red instead.

## Two defects

**1. The bitcode name.** The check found a fat-LTO unit by its
`<cgu>.rcgu.lto.input.bc` and took the post-LTO module to be the same stem
with `.rcgu.bc`. rustc shortens a temp file name that would be too long into
`<16 hex digits>-<tail of the name>`, and where it cuts depends on the
suffix. Measured with rustc 1.99.0 and a crate named
`nano_ros_cpp_ffi_autoware_safety_island_msgs`:

| file | name |
| --- | --- |
| post-LTO module | `nano_ros_cpp_ffi_autoware_safety_island_msgs-<h>.nano_ros_cpp_ffi_…-cgu.0.rcgu.bc` (full) |
| LTO input | `d5f20b411ab9c6d7-ware_safety_island_msgs-<h>.…-cgu.0.rcgu.lto.input.bc` |
| after restriction | `3e73e3047e77bef2-island_msgs-<h>.…-cgu.0.rcgu.lto.after-restriction.bc` |

Swapping the suffix on the shortened input name gives a file that does not
exist, which is the reported `FileNotFoundError`. The same shortening can hit
a ThinLTO `.thin-lto-after-pm.bc` and the link map's `.rcgu.o` names, which
the check matched by exact name too.

**2. The wrong llc.** The check resolved `llc` (and `llvm-dis`) by running
`rustc --print sysroot` in the nano-ros checkout. A project with its own
`rust-toolchain.toml` builds with ITS pin, so in an installed nano-ros, or
in any checkout without that pin, the lookup returned rustup's default
toolchain: llc from stable 1.98.1 (LLVM 22) on 1.99.0 bitcode (LLVM 23)
fails "Not an int attribute".

## Fix

- A codegen unit's files are matched by unit, never by swapping a suffix:
  `resolve_unit` maps a shortened stem to the one full stem ending in its
  kept tail. The full stems come from the dir's own unshortened names and
  its `.rcgu.o` files. The link-map selection uses the same mapping.
- A fat-LTO input whose unit has no post-LTO module (or several) is a
  `ToolMissing` naming the file. Under ruling (A) that is a warning for an
  unmeasured image, not a refusal, and it is never a traceback.
- `bitcode_tool` resolves `llc` and `llvm-dis` from the toolchain the bitcode
  names as its producer (`<LLVM>-rust-<release>-stable`, through
  `rustc +<release>`). `--llvm-bin` still wins. A tool of another LLVM major
  is refused with the toolchain to install; it is never run.

Self-test: the measured names (fat LTO with shortened stages, with the
post-LTO module missing, a shortened ThinLTO after-pm, a shortened module in
the link selection) and the tool choice (an LLVM 22 llc on LLVM 23 bitcode
is refused, an LLVM 23 one is used). Each case was checked against a mutant
that undoes its fix, and fails there.

Reproduced and verified on a minimal fat-LTO staticlib for
`thumbv7em-none-eabihf` built `-Csave-temps` with 1.99.0: the old code
derives a missing file and resolves the `stable` llc from outside the pin;
the new code picks the full post-LTO module and the 1.99.0 llc.
