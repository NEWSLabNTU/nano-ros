# `talker` — mps2-an385-baremetal / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/mps2-an385-baremetal/rust/talker ~/my-talker && cd ~/my-talker
export NROS_REPO_DIR=/path/to/nano-ros   # your nano-ros checkout
nros sync     # generated/ message crates + build/<image>/nros-cargo.toml
nros build    # or: nros build mps2-an385-baremetal
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [mps2-an385-baremetal README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-baremetal/README.md).

`nros build` leaves the image at
`build/mps2-an385-baremetal/target/<triple>/debug/qemu-bsp-talker`
— `<triple>` is the board's Rust target, from the generated settings.

## Config

Board, RMW, domain and locator: `system.toml` beside `Cargo.toml`
(`[image.mps2-an385-baremetal]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them.

Switching board is one edit: the `[image.*] board` line, then `nros sync`.
`Cargo.toml` names no board crate. Its one image-only dependency,
`qemu_bsp_talker_nros_selection = { version = "0.0.0" }`, is a package `nros sync` generates per
image under `build/<image>/nros-selection/` with the board crate, its RMW
and the board's runtime and panic crates (RFC-0098 D6,
[issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md)).

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
