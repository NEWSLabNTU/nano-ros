# zpico-platform-shim

Thin forwarders from zenoh-pico C symbols (`z_clock_now`, `_z_mutex_*`, …) to [nano-ros](https://github.com/NEWSLabNTU/nano-ros) `nros-platform` trait calls. Lets zenoh-pico run under any RTOS that has a `nros-platform-*` crate.

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
