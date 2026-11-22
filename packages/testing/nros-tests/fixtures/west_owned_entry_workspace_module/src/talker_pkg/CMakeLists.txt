cmake_minimum_required(VERSION 3.20.0)
project(talker_pkg)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)
nros_components_register_node(talker_lib
    PLUGIN talker_pkg::Talker
    EXECUTABLE talker
    SHAPE configure
    TYPED)
