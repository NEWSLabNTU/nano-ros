cmake_minimum_required(VERSION 3.22)
project(can_image_probe_c LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

nano_ros_add_executable(can_image_probe_c src/main.c)

install(TARGETS can_image_probe_c DESTINATION lib/${PROJECT_NAME})
ament_package()
