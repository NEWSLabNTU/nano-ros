cmake_minimum_required(VERSION 3.22)
project(nuttx_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# phase-263 C2b — the NuttX (QEMU arm-virt) embedded C workspace entry. The NuttX image
# links the entry INTO the kernel: the embedded link pass (`nros_platform_link_app` →
# `nros_nuttx_build_example`) drives the cargo `nros-nuttx-ffi` kernel build and emits the
# bootable ELF. The cmake side configures HOST-side (no CMAKE_TOOLCHAIN_FILE — the
# cross-compile happens inside the cargo kernel build via cc-rs). Each node component's
# source is cc-rs-compiled for the NuttX target with its OWN -DNROS_PKG_NAME (Option A), so
# multi-node C works. The `nros-nuttx-ffi` Rust entry pushes the guest IP into eth0 via
# SIOCSIFADDR before `app_main()` (issue #130 — mirrors the Rust BoardEntry path; the
# defconfig-baked IP does NOT reach slirp's 10.0.2.2 on its own); the guest then dials the
# host zenoh router at the baked slirp locator. LAUNCH-only.
nano_ros_entry(
    NAME    nuttx_entry
    BOARD   nuttx-qemu-arm
    LAUNCH  "demo_bringup:system.launch.xml"
    LANG    c
    TYPED
    DEPLOY  nuttx-qemu-arm)
