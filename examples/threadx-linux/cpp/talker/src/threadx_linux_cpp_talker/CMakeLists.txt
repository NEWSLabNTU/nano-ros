cmake_minimum_required(VERSION 3.22)
# The ROS 2 tutorial talker as a component (RFC-0043). No `main()`: the
# image's generated entry owns the init, the executor and the spin loop
# (RFC-0122 D10), and the class prefix is PROJECT_NAME (phase-212.L.4).
project(threadx_linux_cpp_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.cpp)

nros_components_register_node(talker_lib
    PLUGIN threadx_linux_cpp_talker::Talker
    EXECUTABLE talker
    SHAPE configure)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(talker_lib PUBLIC std_msgs__nano_ros_cpp)
endif()
