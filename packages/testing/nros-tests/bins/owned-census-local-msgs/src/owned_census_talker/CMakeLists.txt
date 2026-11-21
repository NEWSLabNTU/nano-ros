cmake_minimum_required(VERSION 3.22)
project(owned_census_talker LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# No find_package(owned_census_msgs): the interface packages package.xml
# `<depend>`s on are generated and linked by the verb (issue 2805's consumer
# shape), so the census build resolves them as the cross build does.
nano_ros_add_executable(owned_census_talker src/main.c)
