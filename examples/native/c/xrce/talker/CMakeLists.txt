cmake_minimum_required(VERSION 3.15)
project(c_xrce_talker
    VERSION 0.1.0
    LANGUAGES C
)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# ============================================================================
# nros-c dependency (use -DNANO_ROS_RMW=xrce when configuring)
# ============================================================================

find_package(NanoRos REQUIRED CONFIG)

# ============================================================================
# Executable
# ============================================================================
add_executable(c_xrce_talker src/main.c)
target_link_libraries(c_xrce_talker PRIVATE NanoRos::NanoRos)

# ============================================================================
# Installation (optional)
# ============================================================================
install(TARGETS c_xrce_talker RUNTIME DESTINATION bin)
