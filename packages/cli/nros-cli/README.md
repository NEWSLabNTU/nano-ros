# nros

The `nros` command-line tool — the user-facing entry point to [nano-ros](https://github.com/NEWSLabNTU/nano-ros).

> **Not published.** This crate is `publish = false`; there is no
> `cargo install nros-cli` and no crates.io release. How you get `nros` depends
> on which side of it you are on:
>
> | you are… | you get `nros` by… |
> | --- | --- |
> | **using** nano-ros to build your own project | installing a release: `curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \| sh` |
> | **developing** nano-ros itself | building it from the checkout: `./scripts/bootstrap.sh` (`just setup-cli` is the internal alias, and does not init the CLI submodule) |
>
> That is not a preference: inside a checkout the tree's own build is the only
> correct binary, because a released `nros` emits code that this tree's runtime
> would have to compile
> ([RFC-0090](../../../docs/design/0090-codegen-version-is-the-compatibility-token.md)).
> No release has been cut yet, so today everyone takes the second row —
> `install.sh` says so if you run it early. A *globally* installed `nros`
> shadowing the tree's own binary is a known footgun; see
> [`book/src/internals/cli-in-monorepo.md`](../../../book/src/internals/cli-in-monorepo.md).

```bash
# Contributor path (the one that works today):
git clone https://github.com/NEWSLabNTU/nano-ros && cd nano-ros
./scripts/bootstrap.sh && source activate.sh

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
