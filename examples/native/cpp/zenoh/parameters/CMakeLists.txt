cmake_minimum_required(VERSION 3.16)
project(cpp_parameters
    VERSION 0.1.0
    LANGUAGES CXX
)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# ============================================================================
# nros C++ setup
# ============================================================================

find_package(NanoRos REQUIRED CONFIG)

# ============================================================================
# Executable
# ============================================================================

add_executable(cpp_parameters src/main.cpp)
target_link_libraries(cpp_parameters
    PRIVATE
        NanoRos::NanoRosCpp
)

# ============================================================================
# Installation (optional)
# ============================================================================

install(TARGETS cpp_parameters RUNTIME DESTINATION bin)
