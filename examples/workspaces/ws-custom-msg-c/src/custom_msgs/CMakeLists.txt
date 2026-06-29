cmake_minimum_required(VERSION 3.20)
project(custom_msgs)

# ===========================================================================
# Verbatim upstream ROS 2 msg-package shape — no nano-ros-specific lines.
# Builds unchanged under `colcon build`; the nano-ros build (and `nros ws sync`)
# routes `rosidl_generate_interfaces` to the nano-ros codegen pipeline, emitting
# the per-pkg `generated/custom_msgs` crate the Node pkgs depend on.
# ===========================================================================

find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)

rosidl_generate_interfaces(${PROJECT_NAME}
    msg/Reading.msg
)

ament_export_dependencies(rosidl_default_runtime)
ament_package()
