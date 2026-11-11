cmake_minimum_required(VERSION 3.22)
project(c_safety_listener LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# The `safety` capability is the bringup's `[system] features`
# (`../native_c_safety_listener_bringup/system.toml`): a workspace image imports the
# runtime with those axes before this file runs.

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(c_safety_listener src/main.c)
ament_target_dependencies(c_safety_listener std_msgs)

install(TARGETS c_safety_listener DESTINATION lib/${PROJECT_NAME})
ament_package()
