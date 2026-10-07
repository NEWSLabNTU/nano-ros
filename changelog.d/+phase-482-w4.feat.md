`rclcpp_lifecycle::LifecycleNode` is available in C++, under upstream's name
and include path (`<rclcpp_lifecycle/lifecycle_node.hpp>`). It IS an
`rclcpp::Node`:

- upstream's constructor;
- the `on_configure(const rclcpp_lifecycle::State&)` … `on_shutdown`
  overrides returning `CallbackReturn::SUCCESS` / `FAILURE` / `ERROR`;
- `configure()`, `activate()`, `deactivate()`, `cleanup()`, `shutdown()` and
  `trigger_transition()`, each returning the resulting state;
- `get_current_state()`;
- `create_publisher<M>(topic, qos)`, returning a managed `LifecyclePublisher`
  that sends only while the node is Active.

So a ported lifecycle node's class body compiles unchanged. The REP-2002
services are registered by the constructor.

`nros::LifecycleNode`, the mixin a component bound to its node with
`bind(Node&)`, is deprecated and will be removed in the next release. Derive
from `rclcpp_lifecycle::LifecycleNode` instead and drop the `bind` /
`register_services` / `autostart` calls; a component that self-drives calls
`configure()` and `activate()`.
