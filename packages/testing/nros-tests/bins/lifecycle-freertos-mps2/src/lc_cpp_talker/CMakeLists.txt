cmake_minimum_required(VERSION 3.22)
project(lc_cpp_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# A managed C++ talker (issue 2428's embedded acceptance). It has no lifecycle
# code: the launch file marks it managed, and the runtime gates its publisher
# on the node's state.
nano_ros_auto_add_library(lc_cpp_talker_lib STATIC src/Talker.cpp)

nros_components_register_node(lc_cpp_talker_lib
    PLUGIN lc_cpp_talker::Talker
    EXECUTABLE cpp_talker
    SHAPE configure)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(lc_cpp_talker_lib PUBLIC std_msgs__nano_ros_cpp)
endif()
