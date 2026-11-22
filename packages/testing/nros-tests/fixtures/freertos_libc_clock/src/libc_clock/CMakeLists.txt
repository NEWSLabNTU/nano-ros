# phase-582 W2 (RFC-0141 D6, issue 2691) -- the FreeRTOS libc clock probe.
#
# A ROS 2-shaped program (`ROS2_MAIN`, so it links the toolchain's libstdc++
# the way a ported node does) that reads every libc clock the board's shims
# answer, first with no wall-clock source and then with a stub board clock.
#
# The stub sits at the one wall-clock symbol: `--wrap` redirects every
# reference to `nros_platform_time_now_ns` outside the port's own TU to
# `__wrap_nros_platform_time_now_ns` (src/clock_probe.cpp), which answers the
# port's value until the probe switches the stub on. The shims under test are
# untouched, so what they read is what a board clock would hand them.
cmake_minimum_required(VERSION 3.22)
project(libc_clock LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

nano_ros_add_executable(libc_clock src/clock_probe.cpp ROS2_MAIN)
target_link_options(libc_clock PRIVATE "-Wl,--wrap=nros_platform_time_now_ns")

install(TARGETS libc_clock DESTINATION lib/${PROJECT_NAME})
ament_package()
