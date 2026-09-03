cmake_minimum_required(VERSION 3.22)
project(service_client_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-263 A1 (services, C++) — poll-model AddTwoInts client; republishes the sum on /sum.

nano_ros_auto_add_library(service_client_lib STATIC src/AddClient.cpp)

nros_components_register_node(service_client_lib
    PLUGIN service_client_pkg::AddClient
    EXECUTABLE add_client
    SHAPE configure
    # issue 0965 — what this component CREATES, read off its own
    # source. Absence means "nobody said" and refuses the whole image,
    # so every component in a workspace declares or none derives.
    ENTITIES
             service_client:example_interfaces/srv/AddTwoInts:/add_two_ints
             timer)

if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(service_client_lib
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
