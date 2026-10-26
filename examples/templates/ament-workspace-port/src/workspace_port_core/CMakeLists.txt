# A stock ament_cmake LIBRARY package, the shape Nav2's `nav2_util` has
# (phase-518 W4). A sibling package finds it with `find_package`, and nothing
# below names nano-ros.
cmake_minimum_required(VERSION 3.8)
project(workspace_port_core)

if(NOT CMAKE_CXX_STANDARD)
  set(CMAKE_CXX_STANDARD 17)
endif()

# The warning set Autoware and Nav2 build with, plus Nav2's `-fPIC`, added to
# the whole directory as `nav2_package()` does.
add_compile_options(-Wall -Wextra -Wpedantic -Werror -fPIC)

find_package(ament_cmake REQUIRED)
find_package(std_msgs REQUIRED)

add_library(sequence SHARED src/sequence.cpp)
target_include_directories(sequence PUBLIC
  "$<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/include>"
  "$<INSTALL_INTERFACE:include/${PROJECT_NAME}>")
# The modern spelling of a message dependency.
target_link_libraries(sequence PUBLIC ${std_msgs_TARGETS})

install(TARGETS sequence EXPORT sequence
  ARCHIVE DESTINATION lib
  LIBRARY DESTINATION lib)
install(DIRECTORY include/ DESTINATION include/${PROJECT_NAME})

ament_export_include_directories(include/${PROJECT_NAME})
ament_export_libraries(sequence)
ament_export_dependencies(std_msgs)
ament_export_targets(sequence)
ament_package()
