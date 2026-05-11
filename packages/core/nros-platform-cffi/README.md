# nros-platform-cffi

C vtable adapter for the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) `nros-platform-api` traits. Lets a C-side platform integration register function pointers at runtime so nros can run on RTOSes lacking a dedicated Rust platform crate.

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
