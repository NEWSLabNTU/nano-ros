# nros-platform

Unified platform abstraction crate for [nano-ros](https://github.com/NEWSLabNTU/nano-ros). Re-exports `nros-platform-api` traits and selects the concrete impl crate based on cargo features (`platform-posix`, `platform-nuttx`, `platform-freertos`, `platform-threadx`, `platform-zephyr`).

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
