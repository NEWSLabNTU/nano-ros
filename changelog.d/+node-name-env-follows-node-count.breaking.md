`NROS_NODE_NAME` and `NROS_NODE_NAMESPACE` now apply to every node a Linux process creates, including nodes named in code or by a launch file. Until now they only renamed the executor's own default identity and were silently dropped for any other node.

- `NROS_NODE_NAMESPACE` replaces the namespace of every node. When a node stated a different namespace, a warning says so once per process.
- `NROS_NODE_NAME` renames the node only when the image states that it creates exactly one node (`[image] node_count`, which `nros build` writes from the contract). With two or more nodes, or with no stated count, it is ignored and a notice says why, because one name cannot name several nodes without a collision.

If you relied on `NROS_NODE_NAME` renaming a single-node application built without `nros build`, build it with `nros build` so its node count is stated, or name the node in `system.toml` or with a launch remap. Launch remaps keep applying to a renamed node.
