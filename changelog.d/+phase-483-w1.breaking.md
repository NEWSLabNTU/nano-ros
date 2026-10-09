The C++ API no longer has an `nros::` namespace. Every user-facing name is
defined in `rclcpp::`, `rclcpp_action::` or `rclcpp_lifecycle::`, where ROS 2
keeps it, and there is no `nros::` alias to fall back on. Spell
`nros::Publisher<M>` as `rclcpp::Publisher<M>`, `nros::create_node` as
`rclcpp::create_node`, `nros::GoalResponse` as `rclcpp_action::GoalResponse`
and `nros::State` as `rclcpp_lifecycle::State`.

A few names changed shape as well as namespace, to match upstream:

- The QoS policy enums are `enum class`: write
  `rclcpp::ReliabilityPolicy::Reliable`, not `nros::Reliable`, and
  `rclcpp::LivelinessPolicy::SystemDefault` for what was `LivelinessNone`.
- `nros::init(locator, domain)`, `nros::shutdown()` and `nros::spin()` return
  a `Result`, where upstream's `rclcpp::init`, `rclcpp::shutdown` and
  `rclcpp::spin` do not, so they are `rclcpp::init_in(...)`,
  `rclcpp::shutdown_in()` and `rclcpp::spin_in(...)`.
- The deprecated `nros::LifecycleNode` is gone; derive from
  `rclcpp_lifecycle::LifecycleNode`.

Generated C++ message headers name the new spellings, so the codegen version
is 10 and older generated trees must be regenerated (`nros sync`).
