# Bare-metal C listener (MPS2-AN385, no RTOS)

Subscribes to `std_msgs/String` on `/chatter` and logs `I heard: [...]`. An ordinary nano-ros **C** application on a Cortex-M3 with **no RTOS at
all** — the same program as `examples/workspaces/c/src/listener_pkg/`.

Same shape as [`../talker`](../talker/README.md), whose README explains it: the
link root is Rust (`src/main.rs` boots the board and calls `nros_app_main()`), and
`build.rs` compiles `src/listener.c` plus the message bindings into it. Both of those
files match the talker's but for the `use` line naming the board crate; this
example differs in `package.xml` and
`src/listener.c` only, plus `nros-codegen.toml`. No libc, and no
`printf` substitution — records are built by hand and emitted through
`rclc_log_emit_at`.

`nros-codegen.toml` bounds `std_msgs/String.data` (256): typed delivery deserializes into storage the caller owns, so an unbounded type has no receive bound to size it with (issue 0964), the same file the FreeRTOS sibling carries.

This directory is a one-package workspace (RFC-0122 D3): the program is
`src/baremetal_c_listener/` (its `Cargo.toml`, `build.rs`, `src/main.rs` and the C
source), and `src/baremetal_c_listener_bringup/system.toml` names it as the image's
OWNED entry (`entry = "baremetal_c_listener"`, RFC-0071 D19), so `nros build` builds
that cargo package rather than generating an entry (phase-535 W6).

## Build

```sh
source ./activate.sh
cd examples/mps2-an385-baremetal/c/listener && nros build
```

## Status

Runs end to end under QEMU (issue 1512): `tests/emulator.rs`'s
`test_qemu_baremetal_capi_e2e` pairs it with its peer against a router on the
port its `system.toml` locator names (this cell's `alloc::port`). The locator
reaches the C TU as `NROS_ENTRY_LOCATOR` and the boot config as
`Config::from_image()`.
