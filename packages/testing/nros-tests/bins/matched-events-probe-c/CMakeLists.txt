cmake_minimum_required(VERSION 3.22)
project(matched_events_probe_c LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# issue 2039 — the C API's matched + status-event callbacks against a stock
# ROS 2 peer; `tests/c_matched_events_interop.rs` drives it.
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(matched_events_probe_c src/main.c)
ament_target_dependencies(matched_events_probe_c std_msgs)

install(TARGETS matched_events_probe_c DESTINATION lib/${PROJECT_NAME})
ament_package()
