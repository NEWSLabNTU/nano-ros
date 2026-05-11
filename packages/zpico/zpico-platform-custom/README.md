# zpico-platform-custom

Custom-link adapter (Phase 115.B) — registers a user-supplied `NrosTransportOps` vtable with zenoh-pico so applications can ship a bespoke transport (e.g. shared-memory, IVC, custom UART framing) without modifying zpico-sys.

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
