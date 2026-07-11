cmake_minimum_required(VERSION 3.22)
project(c_service_client_callback LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(c_service_client_callback src/main.c)
ament_target_dependencies(c_service_client_callback example_interfaces std_msgs)

install(TARGETS c_service_client_callback DESTINATION lib/${PROJECT_NAME})
ament_package()
