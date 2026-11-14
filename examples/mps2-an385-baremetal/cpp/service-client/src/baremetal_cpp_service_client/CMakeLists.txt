cmake_minimum_required(VERSION 3.22)
project(baremetal_cpp_service_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# A typed C++ component. It names no board and has no `main()`: the entry
# generated for the bringup's image constructs this class and calls
# `configure(node)` on the image's executor.
nano_ros_add_node(add_client CLASS baremetal_cpp_service_client::AddClient
    SOURCES src/AddClient.cpp)

# The verb generated the package.xml `<depend>` closure as INTERFACE libs
# (`example_interfaces__nano_ros_cpp` etc.) carrying the generated headers' include
# dirs + FFI-glue link. The typed component's messages need them.
target_link_libraries(baremetal_cpp_service_client_add_client_component
    PUBLIC example_interfaces__nano_ros_cpp)
