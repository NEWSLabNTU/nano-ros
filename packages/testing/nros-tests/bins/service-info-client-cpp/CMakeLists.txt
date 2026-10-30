cmake_minimum_required(VERSION 3.22)
project(service_info_client_cpp LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# issue 2148 — the C++ half of the service-client response-info probe:
# `rclcpp::PollClient<S>::take_response(response, request_header)` against a
# stock `rmw_zenoh_cpp` server (interop_e2e case_12).
find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

nano_ros_add_executable(service_info_client_cpp src/main.cpp)
ament_target_dependencies(service_info_client_cpp example_interfaces)

install(TARGETS service_info_client_cpp DESTINATION lib/${PROJECT_NAME})
ament_package()
