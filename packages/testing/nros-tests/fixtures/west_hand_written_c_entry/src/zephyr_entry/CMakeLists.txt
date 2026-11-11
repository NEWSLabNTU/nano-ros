# Issue 2405 -- a HAND-WRITTEN C west application (`[image.zephyr] entry =
# "zephyr_entry"`), the road `nros build` never generates. It does NOT name
# its image: `nros build` passes the image statement (issue 2436), and a raw
# `west build` passes it with -D, so both roads are the same configure. It
# spells its bringup through `..` (issue 2435: that spelling must find the
# workspace too).
cmake_minimum_required(VERSION 3.20.0)
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
