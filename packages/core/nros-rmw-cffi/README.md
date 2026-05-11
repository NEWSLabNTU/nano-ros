# nros-rmw-cffi

C function-table adapter for [nano-ros](https://github.com/NEWSLabNTU/nano-ros) RMW backends. Lets a C-implemented RMW (e.g. `nros-rmw-cyclonedds`, `nros-rmw-xrce-c`) plug into the Rust `nros-rmw` trait surface at runtime.

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
