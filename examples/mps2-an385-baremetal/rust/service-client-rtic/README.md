# `service-client-rtic` — mps2-an385-baremetal / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/mps2-an385-baremetal/rust/service-client-rtic ~/my-service-client-rtic && cd ~/my-service-client-rtic
NROS_REPO_DIR=/path/to/nano-ros nros sync   # msg crates + [patch.crates-io]
cargo build
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [mps2-an385-baremetal README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-baremetal/README.md).

## Config

Board, RMW, domain and locator: `Cargo.toml` →
`[package.metadata.nros.deploy.rtic-mps2-an385]`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
