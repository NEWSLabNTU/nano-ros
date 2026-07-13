cmake_minimum_required(VERSION 3.22)
project(nuttx_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-281 W3-nuttx — the NuttX (QEMU arm-virt) embedded C RT-TIERS workspace entry,
# the C sibling of the C++ `ws-realtime-cpp/src/nuttx_entry` and of the pure-C pub/sub
# `examples/workspaces/c/src/nuttx_entry`, reusing the existing ctrl/telem C tier nodes.
#
# Same entry-as-kernel-link structure as the pure-C nuttx_entry: the NuttX image links the
# entry INTO the kernel (the embedded link pass `nros_platform_link_app` →
# `nros_nuttx_build_example` drives the cargo `nros-nuttx-ffi` kernel build → bootable ELF).
# The cmake side configures HOST-side (no CMAKE_TOOLCHAIN_FILE; the cross-compile happens
# inside the cargo kernel build via cc-rs). Each node component's source is cc-rs-compiled for
# the NuttX target with its OWN -DNROS_PKG_NAME (Option A), so multi-node C works. The
# `nros-nuttx-ffi` Rust entry brings up eth0 (virtio-net) BEFORE `app_main()` (phase-280); the
# guest then dials the host zenoh router at the baked slirp locator (`tcp/10.0.2.2:<port>`).
#
# Because the demo_bringup `system.toml` declares two `[tiers.*]` with `[tiers.*.nuttx]` raw
# SCHED_FIFO priorities, the C entry codegen (board_is_nuttx → use_run_tiers) emits an
# `nros_app_main` calling `NuttxBoard::run_tiers` (RFC-0015 Model 1): one pthread per tier over
# ONE shared session — ctrl (10 ms, high) publishes /ctrl, telem (100 ms, low) publishes /telem.
# The ctrl/telem node pkgs are added by the workspace root as sibling subdirs; the entry's
# LAUNCH names them so the codegen sees their nano_ros_node_register metadata. LAUNCH-only.
nano_ros_add_executable(nuttx_entry
    BOARD   nuttx-qemu-arm
    LAUNCH  "demo_bringup:system.launch.xml"
    LANG    c
    TYPED
    DEPLOY  nuttx-qemu-arm)
