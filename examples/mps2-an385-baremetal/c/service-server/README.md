# Bare-metal C service server (MPS2-AN385, no RTOS)

Serves `example_interfaces/AddTwoInts` on `/add_two_ints`. An ordinary nano-ros **C** application on a Cortex-M3 with **no RTOS at
all** — the same program as `examples/mps2-an385-freertos/c/service-server/`.

Same shape as [`../talker`](../talker/README.md), whose README explains it: the
link root is Rust (`src/main.rs` boots the board and calls `app_main()`), and
`build.rs` compiles `src/service_server.c` plus the message bindings into it. Both of those
files are byte-identical to the talker's; this leaf differs in `package.xml` and
`src/service_server.c` only. No libc, and no
`printf` substitution — records are built by hand and emitted through
`nros_log_emit_at`.

## Build

```sh
source ./activate.sh
nros sync examples/mps2-an385-baremetal/c/service-server
cd examples/mps2-an385-baremetal/c/service-server && cargo build --release
```

## Status

Build-only (issue 1512): it boots in QEMU as far as the session open, and has no
runtime lane until the entry locator reaches a cargo-rooted C image — see the
talker's README.
