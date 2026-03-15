cmake_minimum_required(VERSION 3.16)
project(c_xrce_action_server
    VERSION 0.1.0
    LANGUAGES C
)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# nros-c dependency (use -DNANO_ROS_RMW=xrce when configuring)
find_package(NanoRos REQUIRED CONFIG)

nano_ros_generate_interfaces(example_interfaces SKIP_INSTALL)

add_executable(c_xrce_action_server src/main.c)
target_link_libraries(c_xrce_action_server
    PRIVATE
        example_interfaces__nano_ros_c
        NanoRos::NanoRos
)

install(TARGETS c_xrce_action_server RUNTIME DESTINATION bin)
