cmake_minimum_required(VERSION 3.22)
project(action_client_callback_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-535 W6 (RFC-0122 D2, a VARIANT) — the Fibonacci client on the TYPED
# `rclcpp_action::Client<A>` and its `SendGoalOptions` callbacks (RFC-0041).
# Was the `examples/linux/cpp/action-client-callback` leaf.

nano_ros_auto_add_library(action_client_callback_lib STATIC src/FibCallbackClient.cpp)

nros_components_register_node(action_client_callback_lib
    PLUGIN action_client_callback_pkg::FibCallbackClient
    EXECUTABLE fib_callback_client
    SHAPE configure
)

if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(action_client_callback_lib
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
