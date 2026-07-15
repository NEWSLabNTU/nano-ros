---
id: 199
title: "`just nuttx build-riscv-c` red: ffi image link fails with undefined `std_msgs_msg_string_{init,serialize,get_type_support}`"
status: open
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
