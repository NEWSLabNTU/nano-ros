# nros

The unified [nano-ros](https://github.com/NEWSLabNTU/nano-ros) facade crate. Re-exports `nros-core`, `nros-node`, `nros-rmw`, `nros-serdes`, `nros-platform`, and reaches the RMW backend + platform purely through the vtable seams (`nros-rmw-cffi` / `nros-platform-cffi`).

This is the entry point for application code. See the [book](https://github.com/NEWSLabNTU/nano-ros/tree/main/book) and the [examples directory](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples).

## Features

Phase 248 — `nros` is RMW- and platform-AGNOSTIC: it carries only FUNCTIONAL features. The concrete RMW backend + platform enter the link graph via the board crate, the board-less app's own `nros-rmw-*` dep, or the `nros-c`/`nros-cpp` staticlib root — never an `nros` feature.

- `std` / `alloc` / `no-std`
- `rmw-cffi` (the RMW vtable runtime — the only RMW feature)
- `lending`, `bridge` / `config`, `param-services`, `lifecycle-services`, `safety-e2e`, `stream`, `ffi-sync`
- ROS edition: `ros-humble` / `ros-iron`

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
