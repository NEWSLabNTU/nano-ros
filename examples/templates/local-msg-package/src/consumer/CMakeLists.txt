cmake_minimum_required(VERSION 3.20)
project(consumer LANGUAGES CXX)

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)

# Workspace msg pkgs — smart Find-stub finds them under
# NROS_INTERFACE_SEARCH_PATH (set by the umbrella CMakeLists.txt one
# level up).
find_package(local_msgs REQUIRED)
find_package(extra_msgs REQUIRED)

# AMENT msg pkgs — smart Find-stub falls through to AMENT_PREFIX_PATH
# (the system ROS install). The codegen + linkage path is identical to
# the workspace pkgs above.
find_package(std_msgs REQUIRED)
find_package(geometry_msgs REQUIRED)
find_package(sensor_msgs REQUIRED)

add_executable(consumer src/consumer.cpp)

# Stock-ROS link shape — every pkg exposes `<pkg>::<pkg>` regardless of
# whether the smart Find-stub resolved it from the workspace or AMENT.
target_link_libraries(consumer
    rclcpp::rclcpp
    local_msgs::local_msgs
    extra_msgs::extra_msgs
    std_msgs::std_msgs
    geometry_msgs::geometry_msgs
    sensor_msgs::sensor_msgs
)

ament_package()
