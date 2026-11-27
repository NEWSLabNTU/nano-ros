"Run managed nodes" no longer says a C++ `rclcpp_lifecycle::LifecyclePublisher` drops a message published inside `on_activate`: it opens before `on_activate` runs, so that message goes out.
