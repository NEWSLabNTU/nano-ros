# phase-583 W2 fixture: an owned entry that builds TWO programs, chosen at run
# time (RFC-0071 D20). Both compile the same talker; only the name differs.
cmake_minimum_required(VERSION 3.22)
project(two_programs LANGUAGES C CXX)
set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)
nano_ros_add_executable(prog_a src/main.c)
ament_target_dependencies(prog_a std_msgs)
nano_ros_add_executable(prog_b src/main.c)
ament_target_dependencies(prog_b std_msgs)
install(TARGETS prog_a prog_b DESTINATION lib/${PROJECT_NAME})
ament_package()
