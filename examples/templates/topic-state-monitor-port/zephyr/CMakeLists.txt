# The Zephyr build of the SAME ported node — phase-482 W3.
#
# `../src/topic_state_monitor.cpp` is the ported monitor, unmodified, its own
# `int main(int argc, char** argv)` included. A Zephyr application is the
# kernel's `app` target, which `find_package(Zephyr)` creates and which must
# exist before `project()`; `nano_ros_add_executable` puts the sources there and
# `ament_target_dependencies` follows them. `ROS2_MAIN` renames the program's
# `main` and reaches it through the board's own `main(void)`.
#
# Built for `mps2/an385` (Cortex-M3, QEMU), not `native_sim`: a ported program
# needs the full C++ standard library (`prj.conf`), and on native_sim Zephyr's
# C library is not one the host's libstdc++ can sit on (phase-209 G.2).
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(topic_state_monitor LANGUAGES C CXX)

find_package(nano_ros REQUIRED)
find_package(diagnostic_updater REQUIRED)

nano_ros_add_executable(topic_state_monitor ../src/topic_state_monitor.cpp ROS2_MAIN)
ament_target_dependencies(topic_state_monitor diagnostic_updater)
