# `action-client` — mps2-an385-freertos / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/mps2-an385-freertos/rust/action-client ~/my-action-client && cd ~/my-action-client
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros sync     # generated/ message crates + build/<image>/nros-cargo.toml
nros build    # or: nros build mps2-an385-freertos
```

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [mps2-an385-freertos README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-freertos/README.md).

`nros build` leaves the image at
`build/mps2-an385-freertos/target/<triple>/debug/action-client`
— `<triple>` is the board's Rust target, from the generated settings.

## Config

Board, RMW, domain and locator: `system.toml` beside `Cargo.toml`
(`[image.mps2-an385-freertos]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them.

`Cargo.toml` names no board crate. The board crate, its RMW backend and
its runtime crates come from the `<pkg>_nros_selection` package that
`nros sync` / `nros build` generate under `build/nros/selection/` from
`system.toml` (RFC-0098 D6), so switching board is the `[image.*] board`
line and nothing else
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1305-single-package-board-crate-dep-not-generated.md)).

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
