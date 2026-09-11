# `action-server-rtic` — native / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/native/rust/action-server-rtic ~/my-action-server-rtic && cd ~/my-action-server-rtic
export NROS_REPO_DIR=/path/to/nano-ros   # your nano-ros checkout
nros sync                                # generated/ message crates
cargo build
```

This leaf carries no `system.toml` yet, so it declares no `[image.*]` for
`nros build` to resolve (RFC-0098 D3); cargo drives it until it gains one.

## Run

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`).
The binary lands under `target/debug/`; `cargo run` runs it, after the
`nros sync` above.

## Config

Board, RMW, domain and locator belong in a `system.toml` beside `Cargo.toml`
(`[image.<id>]` + `[system]`, RFC-0098 D3/D5). This leaf has not been
migrated yet, so it still selects its backend with the `rmw-*` Cargo
features in `Cargo.toml` — the spelling RFC-0098 retires.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
