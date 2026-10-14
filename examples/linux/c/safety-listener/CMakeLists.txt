cmake_minimum_required(VERSION 3.22)
project(c_safety_listener LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 261 W5 — declare the E2E-safety capability via the language-neutral
# `NROS_FEATURES` surface (the C/C++ projection of `system.toml`
# `[system].features = ["safety"]`). The root `nros_lower_system_features` lowers
# it to `NROS_SAFETY_E2E=ON` before nros-cpp/nros-c read the option, building
# the zenoh backend with the CRC + validated-recv API. (Was a hand-set
# `NROS_SAFETY_E2E` knob; now the declarative axis the registry owns.)
# Set BEFORE find_package(nano_ros) so the axis reaches the import.
set(NROS_FEATURES "safety")

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(c_safety_listener src/main.c)
ament_target_dependencies(c_safety_listener std_msgs)

install(TARGETS c_safety_listener DESTINATION lib/${PROJECT_NAME})
ament_package()
