cmake_minimum_required(VERSION 3.22)
project(native_entry_robot1 VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 Track C — per-host entry for `robot1`. `HOST robot1` makes the codegen
# (`nros codegen entry --host robot1`) keep only `<node machine="robot1">` (the
# talker) from `multihost.launch.xml`, so this binary bakes talker-only — the C
# parity of the Rust `native_entry_robot1` (`nros::main!(host = "robot1")`).
nano_ros_add_executable(native_entry_robot1
    SOURCES src/main.c
    BOARD   native
    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/multihost_model.yaml"
    TYPED
    HOST    robot1
    DEPLOY  native)
