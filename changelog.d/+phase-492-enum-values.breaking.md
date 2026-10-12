C API: three enums that already carried upstream's type name now take
upstream's value names too (phase-492). Same numbers; rename in C code:
`RCLC_CLOCK_{UNINITIALIZED,ROS_TIME,SYSTEM_TIME,STEADY_TIME}` ->
`RCL_CLOCK_UNINITIALIZED`, `RCL_ROS_TIME`, `RCL_SYSTEM_TIME`, `RCL_STEADY_TIME`;
`RCLC_SEMANTICS_RCLCPP_EXECUTOR` / `RCLC_SEMANTICS_LOGICAL_EXECUTION_TIME` ->
`RCLCPP_EXECUTOR` / `LET`; `RCLC_EXECUTOR_ON_NEW_DATA` / `RCLC_EXECUTOR_ALWAYS`
-> `ON_NEW_DATA` / `ALWAYS`. The C++ binding's `rclcpp::Clock` takes
`RCL_ROS_TIME` etc., as upstream rclcpp does.
