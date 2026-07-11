cmake_minimum_required(VERSION 3.24)
project(c_listener LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(c_listener src/main.c)
ament_target_dependencies(c_listener std_msgs)

install(TARGETS c_listener DESTINATION lib/${PROJECT_NAME})
ament_package()
