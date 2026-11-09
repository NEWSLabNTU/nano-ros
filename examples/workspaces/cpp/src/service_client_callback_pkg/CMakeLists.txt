cmake_minimum_required(VERSION 3.22)
project(service_client_callback_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-535 W6 (RFC-0122 D2, a VARIANT) — the AddTwoInts client whose reply
# arrives through the typed `rclcpp::Client<S>` response callback dispatched at
# spin (RFC-0041). Was the `examples/linux/cpp/service-client-callback` leaf.

nano_ros_auto_add_library(service_client_callback_lib STATIC src/AddCallbackClient.cpp)

nros_components_register_node(service_client_callback_lib
    PLUGIN service_client_callback_pkg::AddCallbackClient
    EXECUTABLE add_callback_client
    SHAPE configure
    # What it creates is stated in
    # service_bringup/launch/service_client_callback.contract.yaml.
)

if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(service_client_callback_lib
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
