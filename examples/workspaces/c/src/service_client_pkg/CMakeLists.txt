cmake_minimum_required(VERSION 3.22)
project(service_client_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)
find_package(std_msgs REQUIRED)

# phase-263 A1 (services, C projection) — typed component using the generated
# example_interfaces/srv/AddTwoInts C bindings (deserialize request / serialize reply). The
# verb generates + links the `example_interfaces__nano_ros_c` interface lib from package.xml.
nano_ros_auto_add_library(service_client_lib STATIC src/AddClient.c)

nros_components_register_node(service_client_lib
    PLUGIN service_client_pkg::AddClient
    EXECUTABLE add_client
    SHAPE configure
    TYPED)
