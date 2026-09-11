cmake_minimum_required(VERSION 3.22)
project(cpp_parameters LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# phase-426 W4 — this example declares parameters on THE store (the executor's
# `nros_params` table, the one the six `rcl_interfaces/srv/*` servers read), so
# it needs the `param_services` capability. Declared through the language-
# neutral `NANO_ROS_FEATURES` surface, which is the C/C++ projection of
# `system.toml` `[system].features`. Set BEFORE find_package(nano_ros) so the
# axis reaches the import.
set(NANO_ROS_FEATURES "param_services")

find_package(nano_ros REQUIRED)

nano_ros_add_executable(cpp_parameters src/main.cpp)

# RFC-0089 §"The C++ std surface is REQUESTED, not discovered": the hosted
# surface is a request, never something a probe discovers from the include
# path. This example asks for it because its `frame_id` parameter is a
# `std::string` — the SCALAR parameter methods reach a freestanding node
# without it (phase-426 W4 moved them out of `NROS_CPP_NODE_HOSTED`), and so
# does the `nros::Seq<double, 8>` sequence below; only the `std::string`
# overloads need this.
target_compile_definitions(cpp_parameters PRIVATE NROS_CPP_STD=1)

install(TARGETS cpp_parameters DESTINATION lib/${PROJECT_NAME})
ament_package()
