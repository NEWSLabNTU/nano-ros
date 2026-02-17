cmake_minimum_required(VERSION 3.16)
project(baremetal_demo C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(NanoRos REQUIRED CONFIG)

# Create executable
add_executable(baremetal_demo
    src/main.c
    src/platform_impl.c
)

# Link libraries
target_link_libraries(baremetal_demo PRIVATE NanoRos::NanoRos)

# Platform-specific libraries not covered by NanoRos::NanoRos
if(UNIX AND NOT APPLE)
    target_link_libraries(baremetal_demo PRIVATE rt)
elseif(APPLE)
    target_link_libraries(baremetal_demo PRIVATE "-framework CoreFoundation")
endif()

# Compiler warnings
target_compile_options(baremetal_demo PRIVATE
    -Wall
    -Wextra
    -Wpedantic
    -Wno-unused-result  # Demo intentionally ignores return values in cleanup paths
)

# Print build info
message(STATUS "")
message(STATUS "Bare-metal Demo Build Configuration:")
message(STATUS "  C Compiler: ${CMAKE_C_COMPILER}")
message(STATUS "  C Standard: C${CMAKE_C_STANDARD}")
message(STATUS "  Build Type: ${CMAKE_BUILD_TYPE}")
message(STATUS "")
