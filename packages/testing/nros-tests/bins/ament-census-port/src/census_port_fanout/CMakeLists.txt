# A stock ament_cmake package (issue 2043): nothing below names nano-ros.
cmake_minimum_required(VERSION 3.8)
project(census_port_fanout)

if(NOT CMAKE_CXX_STANDARD)
  set(CMAKE_CXX_STANDARD 17)
endif()

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)
find_package(std_msgs REQUIRED)

add_executable(census_port_fanout src/fanout.cpp)
ament_target_dependencies(census_port_fanout rclcpp std_msgs)

install(TARGETS census_port_fanout RUNTIME DESTINATION lib/${PROJECT_NAME})

ament_package()
