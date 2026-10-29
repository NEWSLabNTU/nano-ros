`nros new --template <role>` scaffolds a real node for each role of the ROS 2
demos: `talker`, `listener`, `server` and `client` (AddTwoInts), and
`action-server` and `action-client` (Fibonacci), in Rust, C and C++, for a
package, a Zephyr app or a `--component`. Each is the in-tree example of that
role under the package's names, so the C/C++ talker now publishes on a timer
instead of once. The package flags follow `ros2 pkg create`: `--node-name`,
`--dependencies`, `--license`, `--description`, `--maintainer-name`,
`--maintainer-email` and `--destination-directory`. `--use-case`, which wrote
a talker for every value, is refused, naming `--template` (phase-517 W12).
