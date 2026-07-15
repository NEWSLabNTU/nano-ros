---
id: 199
title: "`just nuttx build-riscv-c` red: ffi image link fails with undefined `std_msgs_msg_string_{init,serialize,get_type_support}`"
status: resolved
type: bug
area: nuttx
related: [phase-285, 0165]
---

## Problem

`just nuttx build-riscv-c` (the riscv-nuttx C lane, `fixtures-build.sh
nuttx-riscv c zenoh`) fails at the `nros-nuttx-riscv-ffi` image link:

```
main.c:(.text.std_msgs_msg_string_publish+0x1a): undefined reference to `std_msgs_msg_string_serialize'
main.c:(.text.nros_app_main+0xd4): undefined reference to `std_msgs_msg_string_get_type_support'
main.c:(.text.nros_app_main+0x136): undefined reference to `std_msgs_msg_string_init'
error: could not compile `nros-nuttx-riscv-ffi` (bin "nros-nuttx-ffi") due to 1 previous error
```

The `examples/qemu-riscv-nuttx/c/talker` app TU (`main.c`) compiles and calls
the generated `std_msgs` C bindings, but the generated binding TUs (or the
archive carrying them) never reach the ffi image link.

## Baseline-verified pre-existing

Verified **at HEAD with all phase-285 W3–W6 changes stashed** (2026-07-15): the
failure is identical, so it predates the riscv `run_tiers` work. It was
presumably introduced by a change in the generated-C-binding plumbing since the
lane last ran green in nightly CI (`just nuttx build-all`). Note the arm
sibling `build-c` is green — the break is riscv-lane-specific wiring, not the
codegen itself.

Because of this red, phase-285 W5/W6 deliberately decoupled the new
`build-riscv-rust` recipe from `build-riscv-c` (it self-provisions the rv-virt
kernel), and the riscv-nuttx board stayed **off-matrix** in
`exec_model_matrix.rs` (the C/C++ riscv e2e siblings are deferred on this
issue).

## Fix direction

Compare the arm C lane's generated-binding wiring (how
`std_msgs__nano_ros_c`-style objects/archives get onto the
`nros-nuttx-ffi` image link line) with the riscv lane's; the miss is likely in
the riscv board cmake (`cmake/board/nano-ros-board-nuttx-qemu-riscv.cmake`) or
the fixture row's flags. Once green, revisit riscv C/C++ e2e + the matrix
decision (see 0165's resolution).


## Resolution (2026-07-15)

Exactly the predicted class: `cmake/board/nano-ros-board-nuttx-qemu-riscv.cmake`'s
`nros_board_link_app` was a stale pre-phase-281 mirror of the arm overlay. The
generated `<pkg>__nano_ros_c` interface lib is a HOST-arch static lib of serdes
`.c` TUs, so it can never join the riscv kernel link — phase-281 (arm) added the
`INTERFACE_SOURCES` walk that hands those generated `.c` sources to the cc-rs
cross-compile (landing in the trailing `app_iface` archive), but the riscv
overlay never received the port, so `c_talker_ffi_libs.txt` stayed empty and the
serdes symbols were undefined at the image link.

Fix: ported the three missing blocks from the arm overlay verbatim (riscv triple
aside): the phase-263 C2b component-lib source walk (+ `SOURCE_PKGS`), the issue
0149 one-level descent that surfaces `__nano_ros_(c|cpp)` libs hanging off
component libs, and the phase-281 `_iface_srcs` walk + `INTERFACE_SOURCES`
parameter. The shared `nros_board_common::nuttx_ffi_build::run_nuttx()` already
consumes `APP_INTERFACE_SOURCES` / `APP_EXTRA_SOURCE_PKGS`, so no Rust-side
change was needed.

`just nuttx build-riscv-c` GREEN; the fixed `c_talker` ELF boots on rv-virt and
runs to the expected bare-boot `nros_support_init -> -4` (no router). This is
the same "arm overlay evolved, riscv mirror silently stale" recurrence class as
the pitfalls in `cyclone-cross-and-sizes-header-race-recurrence` — when a phase
touches `nano-ros-board-nuttx-qemu-arm.cmake`'s link_app, port the riscv twin in
the same commit.
