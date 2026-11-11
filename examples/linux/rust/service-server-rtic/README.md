# `service-server-rtic` — linux / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A one-package workspace: `src/native_rtic_service_server` holds the program, and
`src/native_rtic_service_server_bringup` holds the `system.toml` that says what to build. The
program keeps its own `main`, which is what this example demonstrates, so
every image names it as its owned entry (`entry = "native_rtic_service_server"`) and `nros build`
builds it rather than generating one.

```bash
cp -r examples/linux/rust/service-server-rtic ~/my-service-server-rtic && cd ~/my-service-server-rtic
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build native_rtic_service_server_bringup:linux
```

## Run

Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`).

```bash
nros run linux
```

## Config

Board, RMW, domain and locator: `src/native_rtic_service_server_bringup/system.toml`
(`[image.*]` + `[system]`, RFC-0098 D3/D5). No build command carries any of
them. `src/native_rtic_service_server/Cargo.toml` names the board crate it calls into
(`nros-board-linux`); its RMW and edition features come from the
`native_rtic_service_server_nros_derived` adapter crate `nros build` generates from the bringup
(RFC-0071 D13/D19).

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
