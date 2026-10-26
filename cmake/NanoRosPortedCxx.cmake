# NanoRosPortedCxx.cmake — a ported ROS 2 package's C++ is hosted (phase-518).
#
# A ported package uses the C++ standard library (`std::string`,
# `std::shared_ptr`, `<chrono>` literals) through the std-shaped API
# (`NROS_CPP_STD`). A toolchain that compiles C++ `-ffreestanding` (FreeRTOS,
# ThreadX) makes `<string>` a hard `#error` (issue 1240), and no C++ flag undoes
# it (`-fhosted` is C-only). So the flag leaves the ported package's DIRECTORY:
# its C++ is hosted, which is the porting user's choice for that image. The
# nano-ros API itself stays freestanding (RFC-0096 D1), because nano-ros's own
# directories were configured before the package's.
#
# Two roads reach it. `nano_ros_add_executable(... ROS2_MAIN)` calls it for the
# caller's directory. A stock ROS 2 package reaches it through the find modules
# its CMakeLists calls: `find_package(rclcpp)` (and the `rclcpp_*` modules that
# include it, and `rclcpp_components`) runs in the package's directory scope.
# Before phase-518 only the first did it, so a stock package built for FreeRTOS
# got `NROS_CPP_STD` from `rclcpp::rclcpp` and kept `-ffreestanding`, and its
# first `#include <rclcpp/rclcpp.hpp>` failed in `<string>`.

#
# The same directory also gets `std::mutex` where the toolchain's libstdc++ has
# no thread support (arm-none-eabi): `packages/api/nros-cpp/compat/gthreadless`
# holds a `<mutex>` that forwards to the toolchain's and adds the mutex over the
# platform's (phase-518 W4, Nav2's `OdomSmoother`). The Zephyr module puts the
# same directory on its C++ include path.

# CACHE INTERNAL: the macro below runs in the caller's directory, where
# CMAKE_CURRENT_LIST_DIR is the caller's (the `_NROS_ENTRY_DIR` pattern).
set(_NROS_GTHREADLESS_COMPAT_DIR
    "${CMAKE_CURRENT_LIST_DIR}/../packages/api/nros-cpp/compat/gthreadless"
    CACHE INTERNAL "nros-cpp's <mutex> for a libstdc++ without threads")

# nros_ported_cxx_hosted()
#
# A MACRO, so it edits CMAKE_CXX_FLAGS in the scope it is called from: a find
# module's (the caller's directory) or a function's (which then forwards it
# with PARENT_SCOPE). A no-op when the flag is absent (posix, Zephyr).
macro(nros_ported_cxx_hosted)
    if(CMAKE_CXX_FLAGS MATCHES "-ffreestanding")
        string(REPLACE "-ffreestanding" "" CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS}")
        include_directories(BEFORE "${_NROS_GTHREADLESS_COMPAT_DIR}")
    endif()
endmacro()

# nros_ported_ament_package()
#
# phase-518 W4 — what `find_package(ament_cmake)` (and `_auto` / `_ros`) means
# here: this directory is a ported ament package. Every stock package makes
# that call, so it is the one hook that does not depend on which other
# packages it finds. Before it, the hosted switch came only from
# `find_package(rclcpp)`, and a library package that finds message packages
# alone (`workspace_port_core`, the shape of a `*_util` package) kept
# `-ffreestanding` and failed in `<mutex>`.
#
# It also marks the directory (`NROS_PORTED_AMENT_PACKAGE`), so a message
# package found here is generated in C++ instead of only validated
# (`_nros_find_ros_msg_package`): its consumers are this package's C++ targets,
# which may name it only as `${<msg>_TARGETS}`.
macro(nros_ported_ament_package)
    nros_ported_cxx_hosted()
    set(NROS_PORTED_AMENT_PACKAGE TRUE)
endmacro()
