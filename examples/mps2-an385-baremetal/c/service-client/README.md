# Bare-metal C service client (MPS2-AN385, no RTOS)

Calls `example_interfaces/AddTwoInts` on `/add_two_ints` once (`2 + 3`). An ordinary nano-ros **C** application on a Cortex-M3 with **no RTOS at
all** — the same program as `examples/workspaces/c/src/service_client_pkg/`.

Same shape as [`../talker`](../talker/README.md), whose README explains it: the
link root is Rust (`src/main.rs` boots the board and calls `nros_app_main()`), and
`build.rs` compiles `src/service_client.c` plus the message bindings into it. Both of those
files match the talker's but for the `use` line naming the board crate; this
example differs in `package.xml` and
`src/service_client.c` only. No libc, and no
`printf` substitution — records are built by hand and emitted through
`rclc_log_emit_at`.

This directory is a one-package workspace (RFC-0122 D3): the program is
`src/baremetal_c_service_client/` (its `Cargo.toml`, `build.rs`, `src/main.rs` and the C
source), and `src/baremetal_c_service_client_bringup/system.toml` names it as the image's
OWNED entry (`entry = "baremetal_c_service_client"`, RFC-0071 D19), so `nros build` builds
that cargo package rather than generating an entry (phase-535 W6).

## Build

```sh
source ./activate.sh
cd examples/mps2-an385-baremetal/c/service-client && nros build
```

## Status

Runs end to end under QEMU (issue 1512): `tests/emulator.rs`'s
`test_qemu_baremetal_capi_e2e` pairs it with its peer against a router on the
port its `system.toml` locator names (this cell's `alloc::port`). The locator
reaches the C TU as `NROS_ENTRY_LOCATOR` and the boot config as
`Config::from_image()`.
