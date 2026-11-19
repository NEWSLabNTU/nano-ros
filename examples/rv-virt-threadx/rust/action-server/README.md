# `action-server` — rv-virt-threadx / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([example directory layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/rv-virt-threadx/rust/action-server ~/my-action-server && cd ~/my-action-server
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros configure     # generated/ message crates + build/<image>/nros-cargo.toml
nros build    # or: nros build rv-virt-threadx
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [rv-virt-threadx README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/rv-virt-threadx/README.md).

`nros build` leaves the image at
`build/rv-virt-threadx/target/<triple>/debug/rv-virt-threadx-action-server`
— `<triple>` is the board's Rust target, from the generated settings.

## Config

Board, RMW, domain and locator: `system.toml` beside `Cargo.toml`
(`[image.rv-virt-threadx]` + `[system]`). No build command
carries any of them.

Switching board is two edits, not one: the `[image.*] board` line
and the board crate this leaf names in `Cargo.toml`'s `[dependencies]`.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
