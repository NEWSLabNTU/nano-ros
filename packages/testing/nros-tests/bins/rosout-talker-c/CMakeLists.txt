cmake_minimum_required(VERSION 3.22)
project(rosout_talker_c LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# issue 1589 — the C half of the `/rosout` bridge's live-peer probe. Declares
# the `rosout` capability (the C/C++ projection of `system.toml`
# `[system].features = ["rosout"]`) BEFORE find_package(nano_ros), so the
# runtime is built with the bridge rather than its UNSUPPORTED stubs.
set(NANO_ROS_FEATURES "rosout")

find_package(nano_ros REQUIRED)

nano_ros_add_executable(rosout_talker_c src/main.c)
install(TARGETS rosout_talker_c DESTINATION lib/${PROJECT_NAME})

ament_package()
