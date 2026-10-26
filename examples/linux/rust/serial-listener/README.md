# `serial-listener` — linux / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/linux/rust/serial-listener ~/my-serial-listener && cd ~/my-serial-listener
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros sync     # generated/ message crates + build/<image>/nros-cargo.toml
nros build    # or: nros build linux
```

## Run

Needs the host daemon of the RMW `system.toml` names.
`nros build` leaves the binary at `build/linux/target/debug/xrce-serial-listener`.

## Config

Board, RMW, domain and locator: `system.toml` beside `Cargo.toml`
(`[image.linux]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them.

`Cargo.toml` names no board crate. The board crate, its RMW backend and
its runtime crates come from the `<pkg>_nros_selection` package that
`nros sync` / `nros build` generate under `build/nros/selection/` from
`system.toml` (RFC-0098 D6), so switching board is the `[image.*] board`
line and nothing else
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1305-single-package-board-crate-dep-not-generated.md)).

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
