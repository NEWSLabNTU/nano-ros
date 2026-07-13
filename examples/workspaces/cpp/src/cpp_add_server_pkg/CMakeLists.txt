cmake_minimum_required(VERSION 3.22)
project(cpp_add_server_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-263 A1 (services, C++) — typed AddTwoInts server using the TYPED bind_service<Svc,…>
# (generated example_interfaces C++ srv bindings).

nano_ros_add_node(add_server CLASS cpp_add_server_pkg::AddServer src/AddServer.cpp)

# Guard with if(TARGET) (Zephyr whole-archives the generated interface; see talker_pkg).
if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(cpp_add_server_pkg_add_server_component
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
