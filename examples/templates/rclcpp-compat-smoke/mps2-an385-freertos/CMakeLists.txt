# The FreeRTOS (mps2-an385, QEMU) build of the SAME ported node — phase-482 W3.
#
# `../src/talker.cpp` is the rclcpp-idiom smoke node, unmodified, its own
# `int main(int argc, char** argv)` included. This file is the whole port: the
# board comes from `system.toml` beside it, and `ROS2_MAIN` reaches the
# program's `main` through the board's startup (`nros_app_main`).
cmake_minimum_required(VERSION 3.22)
project(rclcpp_compat_smoke LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(diagnostic_updater REQUIRED)

nano_ros_add_executable(rclcpp_compat_smoke ../src/talker.cpp ROS2_MAIN)
ament_target_dependencies(rclcpp_compat_smoke diagnostic_updater)
