cmake_minimum_required(VERSION 3.22)
project(native_c_custom_msg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
find_package(nano_ros REQUIRED)

# Hybrid example: this pkg IS its own interface pkg (member_of_group=
# rosidl_interface_packages in package.xml) AND ships a smoke binary
# (`test_messages`) that exercises the generated C bindings.
# Generate C bindings for THIS pkg's own msg/ dir (RFC-0048 §5).
nano_ros_generate_interfaces(${PROJECT_NAME} LANGUAGE C)

nano_ros_add_executable(test_messages src/main.c)

install(TARGETS test_messages DESTINATION lib/${PROJECT_NAME})
ament_package()
