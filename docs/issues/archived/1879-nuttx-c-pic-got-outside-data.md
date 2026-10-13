---
id: 1879
title: "Every NuttX C image fails its first entity init with -3: the app's C TUs are -fPIC, and the flat kernel never initialises its GOT"
status: resolved
type: bug
area: [nuttx, build, boards]
severity: high
found: 2026-10-14
resolved: 2026-10-14
related: [phase-492, issue-0048]
---

## Measured

`cargo nextest run -p nros-tests --test rtos_e2e -E 'test(Platform__Nuttx)'` on
main `e98d30f977`: 7 pass, 2 fail — `test_rtos_pubsub_e2e` and
`test_rtos_service_e2e`, the `Lang__C` cells. Rust and C++ cells pass. The C
listener boots, creates support and node, then:

    main.c:130 rclc_subscription_init_default(..., "/chatter") -> -3

The service server fails the same way at its first entity init.

`-3` is `RCL_RET_INVALID_ARGUMENT`, which `rclc_subscription_init_with_qos`
returns only for a NULL argument or an empty topic. Disassembly of
`examples/qemu-armv7a-nuttx/c/listener/build-zenoh/c_listener`:

- The call site loads the type support from the GOT:
  `ldr r3, [r7, r3]` with `r7 = .got = 0x40200da0` and offset 0. That slot is
  the image's only GOT entry. Its link-time contents are
  `&std_msgs__msg__String__nros_type` (`0x00680f48`).
- `_sdata = 0x40200000`, `_edata = 0x40200da0`. `.got` begins AT `_edata`. The
  copy loop in `__start` moves `_sdata.._edata` from `_eronly`, and `.got`
  falls outside that range. `dramboot.ld`'s `.data` collects `*(.data*)` and
  not `*(.got*)`. At run time the slot reads 0, so the subscription receives a
  NULL `type_info` and returns -3.

The GOT reference exists because `nros_board_common::nuttx_ffi_build`
compiled the app's C TUs with cc-rs's default `-fPIC`. It turned PIC off for
C++ only. Its comment said "C TUs rely on the default PIC for their NuttX
kernel-symbol references", and nobody had measured that. zenoh-pico's C makes
more NuttX-symbol references than any other code in the image, and it has
always been built `pic = false` (`nros-platform-nuttx/nros-platform.toml`).

## Why it surfaced now

`3c9d5e3033` (phase-492 W7/W8) is the offending commit. It is not the
phase-484 W8 knob work, which was the first suspect. Before that commit, a C
app got its type support from a getter FUNCTION
(`std_msgs_msg_string_get_type_support()`). A direct `bl` uses no GOT, so the
defect stayed latent. W8 made `ROSIDL_GET_MSG_TYPE_SUPPORT` the address of a
`__nros_type` DATA object. Under `-fPIC` an extern data reference goes through
the GOT. The last all-green NuttX run was at `a974d3f13c` (PR #2043), which is
not a descendant of `3c9d5e3033`.

This is a class bug, not a type-support bug. Any extern-data reference from
user C code in a NuttX image read 0.

## Fix

`nuttx_platform_build::flat_kernel_code_model` (`build.pic(false)`) is the one
spelling for every `cc::Build` that feeds the flat-kernel link. That covers the
app C and C++ TUs, the interface sources, the platform port and the entry
seam. An alternative is to put `*(.got*)` inside `.data` in our NuttX fork's
`dramboot.ld`. It would be correct too, but a flat kernel with no loader has
no use for PIC.

## Verified

Rebuilt with `just nuttx build-fixtures-arm`. The C images now contain no
`.got`/`.got.plt` section (`arm-none-eabi-readelf -S`).
`rtos_e2e -E 'test(Platform__Nuttx)'` passes 9/9, up from 7/9; both C cells
fail before the fix. `just ci gate` passes 6/6.
