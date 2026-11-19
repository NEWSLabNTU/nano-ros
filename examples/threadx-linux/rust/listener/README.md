# `listener` — threadx-linux / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([example directory layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/threadx-linux/rust/listener ~/my-listener && cd ~/my-listener
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros configure     # generated/ message crates + build/<image>/nros-cargo.toml
nros build    # or: nros build threadx-linux
```

## Run

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`).
`nros build` leaves the binary at `build/threadx-linux/target/debug/listener`.

## Config

Board, RMW, domain and locator: `system.toml` beside `Cargo.toml`
(`[image.threadx-linux]` + `[system]`). No build command
carries any of them.

`Cargo.toml` names no board crate. The board crate, its RMW backend and
its runtime crates come from the `<pkg>_nros_selection` package that
`nros configure` / `nros build` generate under `build/nros/selection/` from
`system.toml`, so switching board is the `[image.*] board`
line and nothing else.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
