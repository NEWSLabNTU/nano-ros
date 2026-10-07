`--ros-args -p [node:]name:=value` and `--params-file <file>` are honoured, by
`nros::init_with_args` / `nros::Context::new` in Rust and by
`rclcpp::init(argc, argv)` in C++, in any image with a parameter store
(`param-services` / `param-store`).

The value replaces the default when a node declares the parameter, on every
road: C, C++, Rust and the launch file's baked values. The last matching
override wins; `node:` reaches only that node; the text is typed by the
declared default.

A parameter file uses the ROS 2 layout (`<node>: ros__parameters: …`, `/**` for
every node, nested keys become dotted names) and holds scalars. Whatever else a
file contains is refused, naming the line. A value that does not parse as the
declared type is logged at error level and the default stays. Images without a
parameter store keep refusing both flags by name.
