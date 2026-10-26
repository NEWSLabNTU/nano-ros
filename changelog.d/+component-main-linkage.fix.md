A component executable from `rclcpp_components_register_node(... EXECUTABLE ...)` now links on a board; its generated `main` was `extern "C"` and no longer matched the renamed entry point.
