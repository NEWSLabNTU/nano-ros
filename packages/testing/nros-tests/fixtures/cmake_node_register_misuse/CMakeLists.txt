# Issue 1620 — the configure-VERDICT fixture behind
# `cmake_node_register_misuse.rs`.
#
# One project, one CASE per `[[compile_check_fixture]]` row: the build stage
# (`builder = "cmake-configure-verdict"`) configures it with
# `-DNROS_VERDICT_CASE=<row id>`, and `cases/<row id>.cmake` supplies the project
# name and the call under test. The configure's exit status and stderr are the
# recorded verdict the test asserts — most of these must FATAL_ERROR.
#
# The cases are files rather than strings in a script so the row's input
# signature (which hashes this whole directory) moves when a case does.
cmake_minimum_required(VERSION 3.22)

if(NOT DEFINED NROS_VERDICT_CASE)
    message(FATAL_ERROR "configure with -DNROS_VERDICT_CASE=<case> (one of cases/*.cmake)")
endif()

# Sets VERDICT_PROJECT and defines `verdict_body()`.
include("${CMAKE_CURRENT_LIST_DIR}/cases/${NROS_VERDICT_CASE}.cmake")

project(${VERDICT_PROJECT} C CXX)
include("@NANO_ROS_ROOT@/cmake/NanoRosNodeRegister.cmake")

verdict_body()
