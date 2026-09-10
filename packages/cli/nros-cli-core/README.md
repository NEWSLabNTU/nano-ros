# nros-cli-core

Library backing the canonical [`nros` CLI](../nros-cli/). Contains the subcommand dispatch, project scaffolder, config inspector, the `build` driver and the `doctor` health check, codegen commands, and shell-completion generator. (There is no `run` verb: `nros build` ends at an artifact — see [What `nros build` Produces](../../../book/src/user-guide/build-artifacts.md).)

> **Not published.** This crate is `publish = false` and is not on crates.io.
> To *use* nano-ros, install an `nros` release (`scripts/install.sh`); to
> *develop* nano-ros, build the CLI from the checkout with
> `./scripts/bootstrap.sh`. See [`nros-cli/README.md`](../nros-cli/README.md).

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option.

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
