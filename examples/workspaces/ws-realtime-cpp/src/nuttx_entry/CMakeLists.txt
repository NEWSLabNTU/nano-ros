cmake_minimum_required(VERSION 3.22)
project(nuttx_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-281 W3-nuttx — the NuttX (QEMU arm-virt) embedded C++ RT-TIERS workspace entry,
# the FIRST full nuttx link + runtime proof of the W3(nuttx) `NuttxBoard::run_tiers` seam
# (commit 37cfaf728). The C++ sibling of both the pure-C `nuttx_entry` (examples/workspaces/c)
# and the Zephyr W3b `ws-realtime-cpp/src/zephyr_entry`, reusing the existing ctrl/telem C++
# tier nodes.
#
# Same entry-as-kernel-link structure as the pure-C nuttx_entry: the NuttX image links the
# entry INTO the kernel (the embedded link pass `nros_platform_link_app` →
# `nros_nuttx_build_example` drives the cargo `nros-nuttx-ffi` kernel build → bootable ELF).
# The cmake side configures HOST-side (no CMAKE_TOOLCHAIN_FILE; the cross-compile happens
# inside the cargo kernel build via cc-rs). The `nros-nuttx-ffi` Rust entry brings up eth0
# (virtio-net) BEFORE `app_main()` (phase-280); the guest then dials the host zenoh router at
# the baked slirp locator (`tcp/10.0.2.2:<port>`).
#
# Because the demo_bringup `system.toml` declares two `[tiers.*]` with `[tiers.*.nuttx]` raw
# SCHED_FIFO priorities, the C++ entry codegen (board_is_nuttx → use_run_tiers) emits an
# `nros_app_main` calling `NuttxBoard::run_tiers` (RFC-0015 Model 1): one pthread per tier over
# ONE shared session — ctrl (10 ms, high) publishes /ctrl, telem (100 ms, low) publishes /telem.
# The ctrl/telem node pkgs are added by the workspace root as sibling subdirs; the entry's
# LAUNCH names them so the codegen sees their nano_ros_node_register metadata. LAUNCH-only.
nano_ros_add_executable(nuttx_entry
    BOARD   nuttx-qemu-arm
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  nuttx-qemu-arm)
