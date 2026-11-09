cmake_minimum_required(VERSION 3.22)
project(service_client_callback_pkg VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-535 W6 (RFC-0122 D2, a VARIANT) — the AddTwoInts client whose reply
# arrives through a typed response callback dispatched at spin (RFC-0041),
# not a poll. Was the `examples/linux/c/service-client-callback` leaf.
nano_ros_auto_add_library(service_client_callback_lib STATIC src/AddCallbackClient.c)

nros_components_register_node(service_client_callback_lib
    PLUGIN service_client_callback_pkg::AddCallbackClient
    EXECUTABLE add_callback_client
    SHAPE configure
    TYPED)
