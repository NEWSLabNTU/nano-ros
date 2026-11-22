cmake_minimum_required(VERSION 3.22)
project(c_lifecycle_node LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# The `lifecycle` capability is the bringup's `[system] features`
# (`../c_lifecycle_node_bringup/system.toml`): a workspace image imports the
# runtime with those axes before this file runs.

find_package(nano_ros REQUIRED)

nano_ros_add_executable(c_lifecycle_node src/main.c)

install(TARGETS c_lifecycle_node DESTINATION lib/${PROJECT_NAME})
ament_package()
