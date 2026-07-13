cmake_minimum_required(VERSION 3.22)
project(reading_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(custom_msgs REQUIRED)

# Typed component (RFC-0043): the raw `/reading` publisher carries the
# workspace-local type name as a string, so no generated C bindings are needed —
# the CDR payload is hand-encoded (custom_msgs/Reading, see src/ReadingTalker.c).
nano_ros_add_node(reading_talker CLASS reading_talker_pkg::ReadingTalker TYPED src/ReadingTalker.c)
