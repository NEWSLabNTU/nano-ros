cmake_minimum_required(VERSION 3.22)
# Phase 212.L.9 — pkg name = CLASS prefix (L.4 rule).
project(riscv64_threadx_cpp_listener VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.7 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    riscv64-qemu)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Typed raw subscription needs the std_msgs C++ binding. The carrier auto-links
# the generated interface lib (Phase 220.G.2). For cyclonedds, the same call
# generates the idlc type support (selected by NROS_RMW).
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 246 (RFC-0043) — TYPED carrier: the bare-metal riscv64 firmware
# constructs the typed Listener component + `configure(node)` binds a raw
# zero-copy subscription on the executor. The board startup.c dispatches to the
# carrier-emitted `app_main`. No manual `Executor::open` / spin / locator, no
# per-example `main.cpp`.
nano_ros_node_register(
    NAME      listener
    CLASS     riscv64_threadx_cpp_listener::Listener
    LANGUAGE  CPP
    HEADER    Listener.hpp
    TYPED
    SOURCES   src/Listener.cpp
    DEPLOY    riscv64-qemu)

target_link_libraries(riscv64_threadx_cpp_listener_listener_component
    PUBLIC std_msgs__nano_ros_cpp)

nano_ros_deploy(TARGET riscv64-qemu RMW ${NROS_RMW} DOMAIN_ID 0)

# RMW backend (zenoh or cyclonedds) onto the carrier-built firmware ELF.
nano_ros_link_rmw(${PROJECT_NAME} RMW "${NROS_RMW}")
