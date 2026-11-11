# Issue 2405 -- a HAND-WRITTEN C west application (`[image.zephyr] entry =
# "zephyr_entry"`), the road `nros build` never generates. It names its image
# itself, as a hand-written application does, and spells its bringup relative to
# itself (issue 2435: that spelling must find the workspace too).
cmake_minimum_required(VERSION 3.20.0)
get_filename_component(NROS_IMAGE_SYSTEM_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup" ABSOLUTE)
set(NROS_IMAGE zephyr)
set(NROS_IMAGE_LANGUAGE cpp)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zephyr_entry)

find_package(nano_ros REQUIRED)

add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../talker_pkg" talker_pkg)
nano_ros_add_executable(zephyr_entry
    BOARD   zephyr
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup"
    LAUNCH  default
    LANG    c
    TYPED
    DEPLOY  zephyr)
