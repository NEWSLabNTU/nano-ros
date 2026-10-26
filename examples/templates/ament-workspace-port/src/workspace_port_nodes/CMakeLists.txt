# A stock ament_cmake package, written the way ROS 2 packages are (phase-518).
# Nothing below names nano-ros: each line is a shape a ported workspace was
# measured to use (ros2/demos `composition`), and each broke on an RTOS until
# phase-518 W4.
cmake_minimum_required(VERSION 3.8)
project(workspace_port_nodes)

if(NOT CMAKE_CXX_STANDARD)
  set(CMAKE_CXX_STANDARD 17)
endif()

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)
find_package(std_msgs REQUIRED)

# A directory include path, which the program's sources need too.
include_directories(include)

# A component library, SHARED as upstream builds it for class_loader, with a
# definition of its own and a message dependency.
add_library(counter_component SHARED src/counter.cpp)
target_compile_definitions(counter_component PRIVATE "WORKSPACE_PORT_BUILDING_LIBRARY")
ament_target_dependencies(counter_component rclcpp std_msgs)

# The program: a plain executable that links the component with the PLAIN
# signature, as `manual_composition` does.
add_executable(workspace_port src/main.cpp)
target_link_libraries(workspace_port counter_component)
ament_target_dependencies(workspace_port rclcpp)

install(TARGETS counter_component workspace_port
  ARCHIVE DESTINATION lib
  LIBRARY DESTINATION lib
  RUNTIME DESTINATION lib/${PROJECT_NAME})

ament_package()
