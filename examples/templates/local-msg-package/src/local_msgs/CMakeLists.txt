cmake_minimum_required(VERSION 3.20)
project(local_msgs)

# ===========================================================================
# This CMakeLists.txt is the VERBATIM upstream ROS 2 msg package shape — no
# nano-ros-specific lines.  Builds unchanged under `colcon build`; the
# nano-ros build resolves `find_package(ament_cmake)` /
# `find_package(rosidl_default_generators)` via Find-stubs and routes
# `rosidl_generate_interfaces` to the nano-ros codegen pipeline.
# ===========================================================================

find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)
find_package(std_msgs REQUIRED)

rosidl_generate_interfaces(${PROJECT_NAME}
    msg/Greeting.msg
    DEPENDENCIES std_msgs
)

ament_export_dependencies(rosidl_default_runtime)
ament_package()
