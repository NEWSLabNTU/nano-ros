# zpico-sys

FFI bindings + bundled C library for [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico), wired for `no_std` use under [nano-ros](https://github.com/NEWSLabNTU/nano-ros). Capacity limits and feature flags are set at build time via environment variables (`ZPICO_MAX_PUBLISHERS`, `Z_FEATURE_LINK_IVC`, …).

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
