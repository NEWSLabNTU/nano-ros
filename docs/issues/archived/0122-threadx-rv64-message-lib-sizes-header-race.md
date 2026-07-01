---
id: 122
title: "threadx-rv64 Cyclone message-lib TUs race the `nros_c_config_header` mirror (0088/0090/0114 class) — the OBJECT_DEPENDS fix was posix-gated"
status: resolved
type: bug
area: cmake
related: [0088, 0090, 0114, 0121]
resolved_in: "NanoRosGenerateInterfaces.cmake — gate the mirror OBJECT_DEPENDS on the mirror target, not platform==posix"
---

## Resolution

The `examples/qemu-riscv64-threadx/rust/talker` Cyclone fixture (`build-cyclonedds`) failed
compiling its generated message library — `std_msgs__nano_ros_c` TU
`std_msgs_msg_int32.c` read the in-tree **`#error` stub** `nros_config_generated.h` and died with
`SESSION_OPAQUE_U64S … undeclared` (and every other `*_OPAQUE_U64S`). Same per-build-sizes-header
mirror race as issues 0088 / 0090 / 0114: the message `.c` TU compiles before Corrosion's
`nros_c_config_header` mirror custom command populates the header on the include path.

Root cause of the *recurrence*: the 0114 fix in `cmake/NanoRosGenerateInterfaces.cmake` (an
`add_dependencies` + a hard file-level `OBJECT_DEPENDS` on the generated sources) was gated
`if(NANO_ROS_PLATFORM STREQUAL "posix")`, on the assumption "embedded generates the header via a
different path." That is false for the **threadx-qemu-riscv64 cross-Cyclone examples**, which build
the sizes header through the *same* `nros_c_config_header` Corrosion mirror as posix — so they hit
the identical race but were excluded from the fix.

Fix: gate the wiring on the mirror actually existing
(`NROS_C_CONFIG_HEADER_FILE` property + `TARGET nros_c_config_header`) instead of the platform name.
Posix and the threadx/riscv64 cross examples both get the ordering + `OBJECT_DEPENDS`; Zephyr and the
freertos carrier (handled in `NanoRosNodeRegister.cmake`, issue 0090) define no such target/property,
so the guards skip them. Verified: `just threadx_riscv64 build-fixtures` now builds all Cyclone
fixtures including `riscv64_threadx_rust_talker_cyclonedds` — "ThreadX-RV64 test fixtures built."

Surfaced only after issue-0121's sibling fix (cross builds self-provision Cyclone instead of leaking
the host `~/.local` install) let the threadx-rv64 Cyclone graph compile far enough to reach the
message libs.
