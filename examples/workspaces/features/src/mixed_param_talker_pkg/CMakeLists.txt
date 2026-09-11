cmake_minimum_required(VERSION 3.22)
project(mixed_param_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# phase-426 W5 — ONE library, TWO languages, one node's parameters. The C++ TU
# is the component; the C TU beside it declares one parameter and reads the
# other's through `nros_cpp_node_*_param_*`. Both land in the executor's
# `nros_params::ParameterServer`, which is the claim W5 makes and which no
# single-language image can test.
nano_ros_auto_add_library(mixed_param_talker_lib STATIC
    src/ParamTalker.cpp
    src/param_probe.c)

nros_components_register_node(mixed_param_talker_lib
    PLUGIN mixed_param_talker_pkg::ParamTalker
    EXECUTABLE mixed_param_talker
    SHAPE configure)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(mixed_param_talker_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()
