- `nros::StandaloneNode`, `nros::NodeConfig`, `nros::PublisherHandle` and
  `nros::SubscriptionHandle` are no longer re-exported. `nros::Node` is the
  node. Code that wants the transport-less standalone node imports it from
  `nros_node`.
- A ported rclrs program can keep its own `use rclrs::*;`. Rename the
  dependency in `Cargo.toml` (`rclrs = { package = "nros", ... }`), or write
  `use nros as rclrs;`.
