# nros-platform-cffi

Canonical C ABI for the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) platform abstraction. Declares a flat set of `extern "C"` symbols (one per platform capability) that a C-implemented platform port supplies at link time. The Rust `nros_platform_api` traits remain available; this crate's `CffiPlatform` ZST dispatches every trait call to the linked C symbols.

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
