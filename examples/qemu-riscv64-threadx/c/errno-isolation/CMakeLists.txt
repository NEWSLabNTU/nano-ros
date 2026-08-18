cmake_minimum_required(VERSION 3.22)
project(c_errno_isolation LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# issue 0680 — no message dependency on purpose. This fixture asserts a
# property of the C library and the scheduler, not of the RMW, so it must not
# acquire a reason to fail for RMW reasons. It also keeps the image small
# enough to build on a board whose Rust half is mid-refactor.
nano_ros_add_executable(c_errno_isolation src/main.c)

install(TARGETS c_errno_isolation DESTINATION lib/${PROJECT_NAME})
ament_package()
