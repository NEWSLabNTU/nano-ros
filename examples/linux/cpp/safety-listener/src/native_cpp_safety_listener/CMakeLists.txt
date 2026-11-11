cmake_minimum_required(VERSION 3.22)
project(cpp_safety_listener LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# The `safety` capability is the bringup's `[system] features`
# (`../native_cpp_safety_listener_bringup/system.toml`): a workspace image imports the
# runtime with those axes before this file runs.

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(cpp_safety_listener src/main.cpp)
ament_target_dependencies(cpp_safety_listener std_msgs)

install(TARGETS cpp_safety_listener DESTINATION lib/${PROJECT_NAME})
ament_package()
