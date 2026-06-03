cmake_minimum_required(VERSION 3.20)
project(std_msgs)

# ===========================================================================
# This CMakeLists.txt is the VERBATIM upstream ROS 2 msg package shape — no
# nano-ros-specific lines. Same source builds under both `colcon build` (it
# overrides AMENT's std_msgs via colcon's overlay semantics) and the
# nano-ros umbrella one level up (which routes find_package(std_msgs) to
# THIS pkg via NROS_INTERFACE_SEARCH_PATH, shadowing the AMENT one).
# ===========================================================================

find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)

rosidl_generate_interfaces(${PROJECT_NAME}
    msg/Marker.msg
)

ament_export_dependencies(rosidl_default_runtime)
ament_package()
