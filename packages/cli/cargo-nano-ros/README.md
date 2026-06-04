# cargo-nano-ros

Shared codegen library for [nano-ros](https://github.com/NEWSLabNTU/nano-ros) message generation. The canonical user CLI is `nros`; install it from the `nros-cli` crate.

```bash
cargo install nros-cli
nros generate-rust --force
```

This package no longer builds a `cargo nano-ros` command. Existing codegen
internals still use the `cargo_nano_ros` Rust library until that library is
renamed or split.

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option.

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
