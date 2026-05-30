cmake_minimum_required(VERSION 3.20)
project(consumer LANGUAGES CXX)

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)
find_package(local_msgs REQUIRED)
find_package(std_msgs REQUIRED)

add_executable(consumer src/consumer.cpp)

# Stock-ROS link shape — the smart Find-stub emits `local_msgs::local_msgs`
# pointing at the nano-ros codegen interface lib.
target_link_libraries(consumer
    rclcpp::rclcpp
    local_msgs::local_msgs
    std_msgs::std_msgs
)

ament_package()
