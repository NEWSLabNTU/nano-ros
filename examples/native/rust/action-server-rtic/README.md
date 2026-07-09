# `action-server-rtic` — native / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/native/rust/action-server-rtic ~/my-action-server-rtic && cd ~/my-action-server-rtic
NROS_REPO_DIR=/path/to/nano-ros nros sync   # msg crates + [patch.crates-io]
cargo build
```

## Run

Needs a zenoh router (`just native zenohd` in the nano-ros checkout):

```bash
cargo run
```

## Config

Locator and domain come from `NROS_LOCATOR` / `ROS_DOMAIN_ID`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
