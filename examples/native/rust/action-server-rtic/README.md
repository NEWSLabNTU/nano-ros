# `action-server-rtic` — native / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/native/rust/action-server-rtic ~/my-action-server-rtic && cd ~/my-action-server-rtic
export NROS_REPO_DIR=/path/to/nano-ros   # your nano-ros checkout
nros sync     # generated/ message crates + build/<image>/nros-cargo.toml
nros build    # or: nros build native
```

## Run

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`).
`nros build` leaves the binary at `build/native/target/debug/rtic-action-server`.

## Config

Board, RMW, domain and locator: `system.toml` beside `Cargo.toml`
(`[image.native]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them.

`Cargo.toml` names no board crate. The board crate, its RMW backend and
its runtime crates come from the `<pkg>_nros_selection` package that
`nros sync` / `nros build` generate under `build/nros/selection/` from
`system.toml` (RFC-0098 D6), so switching board is the `[image.*] board`
line and nothing else
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1305-single-package-board-crate-dep-not-generated.md)).

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
