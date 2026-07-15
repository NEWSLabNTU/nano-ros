cmake_minimum_required(VERSION 3.22)
project(riscv_nuttx_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# #199 follow-up — the NuttX (QEMU rv-virt, riscv32) embedded C++ RT-TIERS
# workspace entry: the riscv sibling of `src/nuttx_entry` (phase-281 W3-nuttx,
# arm-virt) and the C++ sibling of ws-realtime-c's `src/riscv_nuttx_entry`.
# See `src/nuttx_entry/CMakeLists.txt` for the full entry-as-kernel-link
# rationale — identical here, with the riscv board overlay
# (`nano-ros-board-nuttx-qemu-riscv.cmake`) driving the cargo `nros-nuttx-ffi`
# kernel build to a bootable riscv32 ELF.
#
# The demo_bringup `system.toml` `[tiers.*.nuttx]` table keys on the RTOS, so
# arm and riscv share the same 2-tier plan: the C++ entry codegen
# (board_is_nuttx → use_run_tiers) emits an `nros_app_main` calling
# `NuttxBoard::run_tiers` — ctrl (10 ms, high) publishes /ctrl, telem
# (100 ms, low) publishes /telem over ONE shared session.
nano_ros_add_executable(riscv_nuttx_entry
    BOARD   nuttx-qemu-riscv
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  nuttx-qemu-riscv)
