cmake_minimum_required(VERSION 3.8)
project(robot_interfaces)

find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)
find_package(std_msgs REQUIRED)
find_package(geometry_msgs REQUIRED)
find_package(sensor_msgs REQUIRED)

# Core test packages for parser edge cases
find_package(test_msgs REQUIRED)
find_package(moveit_msgs REQUIRED)
find_package(control_msgs REQUIRED)
find_package(nav2_msgs REQUIRED)

# Comprehensive coverage packages
find_package(tf2_msgs REQUIRED)
find_package(trajectory_msgs REQUIRED)
find_package(diagnostic_msgs REQUIRED)
find_package(composition_interfaces REQUIRED)
find_package(rosbag2_interfaces REQUIRED)

rosidl_generate_interfaces(${PROJECT_NAME}
  # Original interface files
  "msg/RobotStatus.msg"
  "msg/SensorReading.msg"
  "srv/SetMode.srv"
  "action/Navigate.action"

  # Test coverage interface files
  "msg/TestEdgeCases.msg"
  "msg/TestMoveit.msg"
  "msg/TestControl.msg"
  "msg/TestNav.msg"
  "srv/TestComplexService.srv"
  "action/TestComplexAction.action"

  DEPENDENCIES
    std_msgs
    geometry_msgs
    sensor_msgs
    test_msgs
    moveit_msgs
    control_msgs
    nav2_msgs
    tf2_msgs
    trajectory_msgs
    diagnostic_msgs
    composition_interfaces
    rosbag2_interfaces
)

ament_package()
