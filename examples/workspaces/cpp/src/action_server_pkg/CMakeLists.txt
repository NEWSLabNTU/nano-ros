cmake_minimum_required(VERSION 3.22)
project(action_server_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-263 A4 (actions, C++) — typed Fibonacci action server using the COMPONENT seam
# bind_action_server_raw + the generated example_interfaces C++ action bindings (deserialize goal
# / serialize result).

nano_ros_auto_add_library(action_server_lib STATIC src/FibServer.cpp)

nros_components_register_node(action_server_lib
    PLUGIN action_server_pkg::FibServer
    EXECUTABLE fib_server
    SHAPE configure
    # issue 0965 — what this component CREATES, read off its own
    # source. Absence means "nobody said" and refuses the whole image,
    # so every component in a workspace declares or none derives.
    ENTITIES
             action_server:example_interfaces/action/Fibonacci:/fibonacci
             timer)

# Guard with if(TARGET) (Zephyr whole-archives the generated interface; see talker_pkg).
if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(action_server_lib
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
