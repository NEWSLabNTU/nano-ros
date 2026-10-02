# Bare-metal C listener (MPS2-AN385, no RTOS)

Subscribes to `std_msgs/String` on `/chatter` and logs `I heard: [...]`. An ordinary nano-ros **C** application on a Cortex-M3 with **no RTOS at
all** — the same program as `examples/mps2-an385-freertos/c/listener/`.

Same shape as [`../talker`](../talker/README.md), whose README explains it: the
link root is Rust (`src/main.rs` boots the board and calls `app_main()`), and
`build.rs` compiles `src/listener.c` plus the message bindings into it. Both of those
files are byte-identical to the talker's; this leaf differs in `package.xml` and
`src/listener.c` only, plus `nros-codegen.toml`. No libc, and no
`printf` substitution — records are built by hand and emitted through
`nros_log_emit_at`.

`nros-codegen.toml` bounds `std_msgs/String.data` (256): typed delivery deserializes into storage the caller owns, so an unbounded type has no receive bound to size it with (issue 0964), the same file the FreeRTOS sibling carries.

## Build

```sh
source ./activate.sh
nros sync examples/mps2-an385-baremetal/c/listener
cd examples/mps2-an385-baremetal/c/listener && cargo build --release
```

## Status

Build-only (issue 1512): it boots in QEMU as far as the session open, and has no
runtime lane until the entry locator reaches a cargo-rooted C image — see the
talker's README.
