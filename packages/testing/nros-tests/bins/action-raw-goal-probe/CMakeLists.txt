cmake_minimum_required(VERSION 3.22)
project(action_raw_goal_probe LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Same shape as examples/native/c/action-{client,server} deliberately: this is
# the C caller `nros_action_client_send_goal_raw` never had (issue 0454 /
# phase-354 W3), and it must be built by the SAME toolchain path a real C
# consumer uses. A hand-rolled `gcc` line cannot: the opaque-size constants
# (`GUARD_HANDLE_OPAQUE_U64S`, …) come from the generated config that only the
# cmake path's size probe produces.
find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(action_raw_goal_probe src/main.c)
ament_target_dependencies(action_raw_goal_probe example_interfaces std_msgs)

install(TARGETS action_raw_goal_probe DESTINATION lib/${PROJECT_NAME})
ament_package()
