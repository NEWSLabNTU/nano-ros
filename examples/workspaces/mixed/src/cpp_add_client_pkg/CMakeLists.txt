cmake_minimum_required(VERSION 3.22)
project(cpp_add_client_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-263 A1 (services, C++) — poll-model AddTwoInts client; republishes the sum on /sum.

nano_ros_add_node(add_client CLASS cpp_add_client_pkg::AddClient src/AddClient.cpp)

if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(cpp_add_client_pkg_add_client_component
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
