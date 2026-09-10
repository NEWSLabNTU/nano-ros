# cargo-nano-ros

Shared codegen library for [nano-ros](https://github.com/NEWSLabNTU/nano-ros) message generation. The canonical user CLI is `nros`, built from the `nros-cli` crate in this repo.

> **Not published.** This crate is `publish = false`; there is no
> `cargo install`. To *use* nano-ros, install an `nros` release
> (`scripts/install.sh`); to *develop* nano-ros, build the CLI from the checkout
> with `./scripts/bootstrap.sh`, then `source activate.sh` to put `nros` on
> `PATH`. See [`nros-cli/README.md`](../nros-cli/README.md) for why the two
> paths are not interchangeable.

```bash
nros generate-rust --force
```

This package no longer builds a `cargo nano-ros` command. Existing codegen
internals still use the `cargo_nano_ros` Rust library until that library is
renamed or split.

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option.

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
