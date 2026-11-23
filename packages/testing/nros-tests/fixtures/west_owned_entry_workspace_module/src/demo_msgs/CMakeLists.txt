# Issue 2876 -- a workspace message package, in the stock ROS 2 shape. The
# owned entry finds it through `NROS_INTERFACE_SEARCH_PATH`, which its own
# CMakeLists does not set: `nros build` passes it on the west line.
cmake_minimum_required(VERSION 3.20)
project(demo_msgs)

find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)

rosidl_generate_interfaces(${PROJECT_NAME}
  msg/Ping.msg
)

ament_export_dependencies(rosidl_default_runtime)
ament_package()
