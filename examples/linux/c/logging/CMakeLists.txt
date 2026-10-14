cmake_minimum_required(VERSION 3.22)
project(c_logging LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
find_package(nano_ros REQUIRED)

# No msg deps; pure `nros::init` + logger demo.
nano_ros_add_executable(c_logging src/main.c)

install(TARGETS c_logging DESTINATION lib/${PROJECT_NAME})
ament_package()
