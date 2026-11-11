cmake_minimum_required(VERSION 3.22)
project(c_parameters LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# The `param_services` capability is the bringup's `[system] features`
# (`../c_parameters_bringup/system.toml`): a workspace image imports the
# runtime with those axes before this file runs.

find_package(nano_ros REQUIRED)

nano_ros_add_executable(c_parameters src/main.c)

install(TARGETS c_parameters DESTINATION lib/${PROJECT_NAME})
ament_package()
