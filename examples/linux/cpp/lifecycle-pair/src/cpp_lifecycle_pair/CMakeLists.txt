cmake_minimum_required(VERSION 3.22)
project(cpp_lifecycle_pair LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# The `lifecycle` capability is the bringup's `[system] features`
# (`../cpp_lifecycle_pair_bringup/system.toml`).

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(cpp_lifecycle_pair src/main.cpp)
ament_target_dependencies(cpp_lifecycle_pair std_msgs)

install(TARGETS cpp_lifecycle_pair DESTINATION lib/${PROJECT_NAME})
ament_package()
