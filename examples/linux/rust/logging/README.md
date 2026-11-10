# `logging` — linux / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A one-package workspace: `src/native_logging` holds the program, and
`src/native_logging_bringup` holds the `system.toml` that says what to build. The
program keeps its own `main`, which is what this example demonstrates, so
every image names it as its owned entry (`entry = "native_logging"`) and `nros build`
builds it rather than generating one.

```bash
cp -r examples/linux/rust/logging ~/my-logging && cd ~/my-logging
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build native_logging_bringup:linux
```

## Run

```bash
nros run linux
```

## Config

Board: `src/native_logging_bringup/system.toml`
(`[image.*]` + `[system]`, RFC-0098 D3/D5). No build command carries any of
them.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
