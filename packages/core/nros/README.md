# nros

The unified [nano-ros](https://github.com/NEWSLabNTU/nano-ros) facade crate. Re-exports `nros-core`, `nros-node`, `nros-rmw`, `nros-serdes`, `nros-platform`, and the RMW backend selected by feature flag (`rmw-zenoh`, `rmw-xrce`, `rmw-cyclonedds`).

This is the entry point for application code. See the [book](https://github.com/NEWSLabNTU/nano-ros/tree/main/book) and the [examples directory](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples).

## Features

- `std` / `alloc` / `no-std`
- RMW backends: `rmw-zenoh`, `rmw-xrce`, `rmw-xrce-cffi`, `rmw-cyclonedds`
- Platforms: `platform-posix`, `platform-nuttx`, `platform-freertos`, `platform-threadx`, `platform-zephyr`

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
