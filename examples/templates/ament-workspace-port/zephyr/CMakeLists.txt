# The Zephyr (mps2/an385, QEMU) build of the SAME workspace (phase-518 W4).
#
# A Zephyr application whose sources are the workspace's packages: Zephyr owns
# the build, so it is found first, and the board comes from `system.toml`
# beside this file. The packages under `../src` are unchanged.
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(ament_workspace_port LANGUAGES C CXX)
find_package(nano_ros REQUIRED)
# The workspace's packages, which a sibling's `find_package` resolves to.
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../src")
nros_workspace_interfaces()
add_subdirectory(../src/workspace_port_core workspace_port_core)
add_subdirectory(../src/workspace_port_nodes workspace_port_nodes)
