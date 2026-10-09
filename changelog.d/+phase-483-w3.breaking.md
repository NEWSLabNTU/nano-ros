A Rust component's `register` now gets the same `nros::Node` a standalone
program gets from `executor.create_node(name)`. Before, `ctx.create_node(...)`
returned a `DeclaredNode`, a different type, so node code could not be shared
between a component and a program.

- The constructors that declare an entity by stable id and bind its callback by
  name (`create_publisher_for_topic`, `create_timer_for_callback_name`,
  `callback_for_name`, …) are now methods of the `nros::DeclarativeNode` trait.
  Add `use nros::DeclarativeNode;` (or use `nros::prelude::*`) where a component
  calls them.
- Four explicit-id forms would otherwise have shadowed rclrs's names on `Node`,
  so they are now `declare_*`: `declare_publisher`, `declare_subscription`,
  `declare_timer` and `declare_action_server` (plus their `_with_qos`,
  `_on_clock` and client siblings).
- The component API now needs the `rmw-cffi` feature, because registration
  creates real nodes.
- `nros::record_node_metadata` takes the executor as well as the recorder.
- `DeclaredNode`, `NodeRuntimeAdapter`, `RuntimeNodeRecord`,
  `DeclaredNodeRuntime`, `NodeExecutorRuntime` and `MISSING_NODE_EXPORT_ERROR`
  are gone.
