# `service-client-rtic` — mps2-an385-baremetal / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/mps2-an385-baremetal/rust/service-client-rtic ~/my-service-client-rtic && cd ~/my-service-client-rtic
export NROS_REPO_DIR=/path/to/nano-ros   # your nano-ros checkout
nros sync     # generated/ message crates + build/<image>/nros-cargo.toml
nros build    # or: nros build mps2-an385-baremetal
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [mps2-an385-baremetal README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-baremetal/README.md).

`nros build` leaves the image at
`build/mps2-an385-baremetal/target/<triple>/debug/qemu-rtic-service-client`
— `<triple>` is the board's Rust target, from the generated settings.

## Config

Board, RMW, domain and locator: `system.toml` beside `Cargo.toml`
(`[image.mps2-an385-baremetal]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them.

Switching board is two edits today, not one: the `[image.*] board` line
and the board crate this leaf names in `Cargo.toml`'s `[dependencies]`. A
single-package leaf is its own entry, and RFC-0098 D6's generated board
dependency reaches only a workspace entry — leave the two disagreeing and
`nros sync` reports success while the build fails in your own crate
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md)).

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
