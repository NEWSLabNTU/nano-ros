cmake_minimum_required(VERSION 3.22)
project(cpp_fib_server_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-263 A4 (actions, C++) — typed Fibonacci action server using the COMPONENT seam
# bind_action_server_raw + the generated example_interfaces C++ action bindings (deserialize goal
# / serialize result).

nano_ros_add_node(fib_server CLASS cpp_fib_server_pkg::FibServer src/FibServer.cpp)

# Guard with if(TARGET) (Zephyr whole-archives the generated interface; see talker_pkg).
if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(cpp_fib_server_pkg_fib_server_component
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
