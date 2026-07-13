cmake_minimum_required(VERSION 3.22)
project(qos_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Typed component (RFC-0043): the raw `/chatter` subscription carries the type
# name as a string and hand-decodes the CDR payload (std_msgs/Int32). The
# differentiator vs the committed c_listener_pkg is the QoS: this declares the
# SAME non-default profile (reliable + transient-local + keep-last-10) as the
# talker so the QoS-matched endpoints connect (see src/QosListener.c).
nano_ros_add_node(qos_listener CLASS qos_listener_pkg::QosListener TYPED src/QosListener.c)
