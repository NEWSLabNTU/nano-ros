# SPDX-License-Identifier: Apache-2.0
# phase-263 C2c — the Zephyr (native_sim) embedded C++ workspace entry, the C++ sibling of
# the C2d C entry. Same entry-as-app structure: this CMakeLists IS a Zephyr app (built by
# west), and nano_ros_add_executable(BOARD zephyr …) is the C/C++ analog of zephyr-lang-rust's
# rust_cargo_application() — it wires the generated multi-node entry TU (an int main(void)
# driving ZephyrBoard::run_components) into the `app` target. The cpp node pkgs use typed
# std_msgs::msg::Int32, so their nros_find_interfaces generates the C++ headers (self-contained
# via the codegen tool the west lane passes); the entry auto-links them. LAUNCH-only.
#
#   west build -b native_sim/native/64 src/zephyr_entry \
#     -- -DCONF_FILE="prj.conf;prj-zenoh.conf;<nsos>.conf" \
#        -DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:<port>" -D_NANO_ROS_CODEGEN_TOOL=<nros>

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_workspace_entry_cpp)

# RFC-0048 ament shape (287-W6): on Zephyr `find_package(nano_ros)` supplies
# the verbs + entry/register machinery WITHOUT re-importing the runtime — the
# nano-ros west module (find_package(Zephyr)) already provides NanoRos::*.
find_package(nano_ros REQUIRED)

# Build the cpp node component libs the launch XML names (talker + listener). Their
# nano_ros_node_register has no DEPLOY → component-only; the entry's sidecar links them.
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../talker_pkg" talker_pkg)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../listener_pkg" listener_pkg)

nano_ros_add_executable(zephyr_entry
    BOARD   zephyr
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  zephyr)
