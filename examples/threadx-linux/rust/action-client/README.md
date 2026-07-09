# `action-client` — threadx-linux / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/threadx-linux/rust/action-client ~/my-action-client && cd ~/my-action-client
NROS_REPO_DIR=/path/to/nano-ros nros sync   # msg crates + [patch.crates-io]
cargo build
```

## Run

Needs a zenoh router (`just native zenohd` in the nano-ros checkout):

```bash
cargo run
```

## Config

Board, RMW, domain and locator: `Cargo.toml` →
`[package.metadata.nros.deploy.threadx-linux]`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
