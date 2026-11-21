# A stock ROS 2 message package: nano-ros routes rosidl_generate_interfaces()
# to its own codegen.
cmake_minimum_required(VERSION 3.20)
project(owned_census_msgs)

find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)

rosidl_generate_interfaces(${PROJECT_NAME}
  msg/Tally.msg
)

ament_export_dependencies(rosidl_default_runtime)
ament_package()
