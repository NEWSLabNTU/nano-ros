cmake_minimum_required(VERSION 3.22)
project(contract_monitor_cpp LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# phase-462 W1 (RFC-0052) -- the C++ twin of bins/contract-monitor's
# `contract-monitor-pub`. Same shape as examples/native/cpp/talker
# deliberately: it must be built by the SAME toolchain path a real C++
# consumer uses, because the opaque-size constants (`NROS_PUBLISHER_SIZE`,
# which this wave grew by one pointer) come from the generated config that
# only the cmake path's size probe produces.
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(contract_monitor_cpp src/main.cpp)
ament_target_dependencies(contract_monitor_cpp std_msgs)

install(TARGETS contract_monitor_cpp DESTINATION lib/${PROJECT_NAME})
ament_package()
