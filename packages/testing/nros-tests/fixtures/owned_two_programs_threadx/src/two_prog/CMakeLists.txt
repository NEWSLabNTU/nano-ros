# phase-583 (RFC-0071 D20) census-fold fixture: ONE owned entry package that
# builds TWO programs on a cross board (threadx-linux), the second with more
# publishers and timers than the first. Its sizing descriptor must hold the
# larger program's counts.
cmake_minimum_required(VERSION 3.22)
project(two_prog LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(prog_small src/small.c)
ament_target_dependencies(prog_small std_msgs)
nano_ros_add_executable(prog_large src/large.c)
ament_target_dependencies(prog_large std_msgs)
# Issue 2899 -- a sibling workspace package, NOT added here: whoever
# configures this package (the workspace, the probe, the census) adds it first.
target_link_libraries(prog_large PRIVATE two_prog_timing)

install(TARGETS prog_small prog_large DESTINATION lib/${PROJECT_NAME})
ament_package()
