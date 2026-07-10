# nros

The `nros` command-line tool — the user-facing entry point to [nano-ros](https://github.com/NEWSLabNTU/nano-ros).

> **Not published.** This crate is `publish = false`; there is no
> `cargo install nros-cli` and no crates.io release. Build it from a nano-ros
> checkout — `just setup-cli` produces `packages/cli/target/release/nros`, and
> `source activate.sh` puts it on `PATH`. A *globally* installed `nros` shadowing
> the tree's own binary is a known footgun; see
> [`book/src/internals/cli-in-monorepo.md`](../../../book/src/internals/cli-in-monorepo.md).

```bash
git clone https://github.com/NEWSLabNTU/nano-ros && cd nano-ros
just setup-cli && source activate.sh

nros new my-project --platform freertos --rmw zenoh --lang c talker
nros generate rust
nros setup freertos
nros doctor
nros board list
```

Thin binary on top of `nros-cli-core`.

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option.

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
