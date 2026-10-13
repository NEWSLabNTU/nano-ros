C++ API: `rclcpp::IntegrityStatus` names the E2E integrity status a safety
subscription delivers (a typedef of the FFI struct), and
`rclcpp::Node::start_parameter_services()` serves a node's parameters on the
`rcl_interfaces` services. C++ examples no longer include
`<nros/nros_cpp_ffi.h>` or call `nros_cpp_*` functions (phase-499 W4).
