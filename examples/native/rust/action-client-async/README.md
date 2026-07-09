# `action-client-async` — native / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/native/rust/action-client-async ~/my-action-client-async && cd ~/my-action-client-async
NROS_REPO_DIR=/path/to/nano-ros nros sync   # msg crates + [patch.crates-io]
cargo build
```

## Run

Needs a zenoh router (`just native zenohd` in the nano-ros checkout):

```bash
cargo run
```

## Config

RMW is a Cargo feature (`--features rmw-zenoh | rmw-cyclonedds | rmw-xrce`);
locator and domain come from `NROS_LOCATOR` / `ROS_DOMAIN_ID`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
