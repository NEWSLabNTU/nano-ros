# Issue 2862 -- the image's OWNED west entry: the C talker program of
# ../c_talker as a Zephyr west application. It states no DEPLOY/BOARD (issue
# 2861). Its bringup declares no component, so the image is sized by the
# census of this program: a host build of the package, run once.
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zephyr_c_talker LANGUAGES C CXX)
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)
nano_ros_add_executable(c_talker "${CMAKE_CURRENT_SOURCE_DIR}/../c_talker/src/main.c")
ament_target_dependencies(c_talker std_msgs)
