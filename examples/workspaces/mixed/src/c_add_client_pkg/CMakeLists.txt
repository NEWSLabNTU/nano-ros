cmake_minimum_required(VERSION 3.22)
project(c_add_client_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)
find_package(std_msgs REQUIRED)

# phase-263 A1 (services, C projection) — typed component using the generated
# example_interfaces/srv/AddTwoInts C bindings (deserialize request / serialize reply). The
# auto-link sidecar links the generated `example_interfaces__nano_ros_c` interface lib.

nano_ros_add_node(add_client CLASS c_add_client_pkg::AddClient TYPED src/AddClient.c)
