cmake_minimum_required(VERSION 3.22)
project(c_parameters LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# phase-426 W4 — this example declares parameters on THE store (the executor's
# `nros_params` table, the one the six `rcl_interfaces/srv/*` servers read), so
# it needs the `param_services` capability. Declared through the language-
# neutral `NANO_ROS_FEATURES` surface, which is the C/C++ projection of
# `system.toml` `[system].features`. Set BEFORE find_package(nano_ros) so the
# axis reaches the import.
set(NANO_ROS_FEATURES "param_services")

find_package(nano_ros REQUIRED)

nano_ros_add_executable(c_parameters src/main.c)

install(TARGETS c_parameters DESTINATION lib/${PROJECT_NAME})
ament_package()
