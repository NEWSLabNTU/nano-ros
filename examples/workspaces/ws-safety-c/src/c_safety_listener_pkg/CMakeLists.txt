cmake_minimum_required(VERSION 3.22)
project(c_safety_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Generates the std_msgs C bindings (serialize/deserialize); the auto-link
# sidecar links the generated `std_msgs__nano_ros_c` interface lib.

# C safety-listener component: registers a validated subscription on /chatter
# (via nros_cpp_subscription_register_validated), validates the E2E CRC on
# each message, and republishes the running CRC-valid count on /safe_ok via
# the generated std_msgs/Int32 serializer.
# Requires NANO_ROS_SAFETY_E2E=ON (from [system].features = ["safety"]).
nano_ros_add_node(safe_listener CLASS c_safety_listener_pkg::Listener TYPED src/SafetyListener.c)
