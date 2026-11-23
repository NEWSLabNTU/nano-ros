# Issue 2861 -- an OWNED west entry (`[image.zephyr] entry = "zephyr_entry"`)
# that states neither DEPLOY nor BOARD: the image already says where it runs,
# and `nros build` states the image (issue 2436). It depends on `support_pkg`,
# a workspace Zephyr module, whose Kconfig symbol its prj.conf sets. Before the
# fix the configure stopped on the empty board key, and with the symbol set,
# on "undefined symbol DEMO_SUPPORT".
#
# Issue 2876 -- it also depends on `demo_msgs`, a workspace message package,
# and sets no `NROS_INTERFACE_SEARCH_PATH`: `nros build` passes the
# workspace's `src/` on the west line, as a generated app writes it. Before the
# fix `find_package(demo_msgs)` found nothing.
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zephyr_entry)

find_package(nano_ros REQUIRED)
find_package(demo_msgs REQUIRED)

add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../talker_pkg" talker_pkg)
nano_ros_add_executable(zephyr_entry
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup"
    LAUNCH  default
    LANG    c
    TYPED)
