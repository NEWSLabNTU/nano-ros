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

# nros_ported_cxx_hosted()
#
# A MACRO, so it edits CMAKE_CXX_FLAGS in the scope it is called from: a find
# module's (the caller's directory) or a function's (which then forwards it
# with PARENT_SCOPE). A no-op when the flag is absent (posix, Zephyr).
macro(nros_ported_cxx_hosted)
    if(CMAKE_CXX_FLAGS MATCHES "-ffreestanding")
        string(REPLACE "-ffreestanding" "" CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS}")
    endif()
endmacro()
