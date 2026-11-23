cmake_minimum_required(VERSION 3.22)
project(c_lifecycle_pair LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# The `lifecycle` capability is the bringup's `[system] features`
# (`../c_lifecycle_pair_bringup/system.toml`).

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(c_lifecycle_pair src/main.c)
ament_target_dependencies(c_lifecycle_pair std_msgs)

install(TARGETS c_lifecycle_pair DESTINATION lib/${PROJECT_NAME})
ament_package()
