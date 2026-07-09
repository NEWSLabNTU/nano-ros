# `action-client-rtic` — stm32f4 / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/stm32f4/rust/action-client-rtic ~/my-action-client-rtic && cd ~/my-action-client-rtic
NROS_REPO_DIR=/path/to/nano-ros nros sync   # msg crates + [patch.crates-io]
cargo build
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [stm32f4 README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/stm32f4/README.md).

## Config

Locator and domain come from `NROS_LOCATOR` / `ROS_DOMAIN_ID`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
