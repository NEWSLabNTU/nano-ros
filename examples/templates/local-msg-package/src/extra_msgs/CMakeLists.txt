cmake_minimum_required(VERSION 3.20)
project(extra_msgs)

find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)
find_package(local_msgs REQUIRED)

rosidl_generate_interfaces(${PROJECT_NAME}
    msg/Echo.msg
    DEPENDENCIES local_msgs
)

ament_export_dependencies(rosidl_default_runtime)
ament_package()
