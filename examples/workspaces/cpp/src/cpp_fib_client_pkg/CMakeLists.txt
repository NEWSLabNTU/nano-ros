cmake_minimum_required(VERSION 3.22)
project(cpp_fib_client_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-263 A4 (actions, C++) — poll-model Fibonacci action client using create_action_client_raw
# + the raw send_goal_async / get_result_async / poll FFI (generated example_interfaces C++ action
# bindings: serialize goal / deserialize result). PRINT-ONLY (one interface pkg, no dedup gap).

nano_ros_add_node(fib_client CLASS cpp_fib_client_pkg::FibClient src/FibClient.cpp)

if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(cpp_fib_client_pkg_fib_client_component
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
