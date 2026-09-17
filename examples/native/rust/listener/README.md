# `listener` — native / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

```bash
cp -r examples/native/rust/listener ~/my-listener && cd ~/my-listener
export NROS_REPO_DIR=/path/to/nano-ros   # your nano-ros checkout
nros sync     # generated/ message crates + build/<image>/nros-cargo.toml
nros build    # or: nros build native
```

## Run

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`).
`nros build` leaves the binary at `build/native/target/debug/listener`.

## Config

Board, RMW, domain and locator: `system.toml` beside `Cargo.toml`
(`[image.native]` + `[system]`, RFC-0098 D3/D5). No build command
carries any of them.

Switching board is two edits today, not one: the `[image.*] board` line
and the board crate this leaf names in `Cargo.toml`'s `[dependencies]`. A
single-package leaf is its own entry, and RFC-0098 D6's generated board
dependency reaches only a workspace entry — leave the two disagreeing and
`nros sync` reports success while the build fails in your own crate
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md)).

What the image KEEPS: `system.contract.yaml` beside `system.toml` ([RFC-0100](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0100-rmw-agnostic-sizing-model.md) D3) — the per-endpoint QoS
no probe of the code can answer. `nros sync` joins it onto what this leaf
registers and writes `build/nros/sizing/<image>.toml`, which each backend sizes
its own pools from; `just mem-report <binary>` is where the difference shows.
Delete the file and the image builds byte-identically to one that never had it.

Write the endpoint's name ABSOLUTELY and exactly as the code registers it. A
contract states RESOLVED names, so a relative spelling or a remapped node is one
the build refuses to match rather than guess at — it names the endpoint and the
image keeps its configured pools.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
