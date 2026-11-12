# Bare-metal C action server (MPS2-AN385, no RTOS)

Serves `example_interfaces/Fibonacci` on `/fibonacci`. An ordinary nano-ros **C** application on a Cortex-M3 with **no RTOS at
all** — the same program as `examples/workspaces/c/src/action_server_pkg/`.

Same shape as [`../talker`](../talker/README.md), whose README explains it: the
link root is Rust (`src/main.rs` boots the board and calls `nros_app_main()`), and
`build.rs` compiles `src/action_server.c` plus the message bindings into it. Both of those
files match the talker's but for the `use` line naming the board crate; this
example differs in `package.xml` and
`src/action_server.c` only. No libc, and no
`printf` substitution — records are built by hand and emitted through
`rclc_log_emit_at`.

This directory is a one-package workspace (RFC-0122 D3): the program is
`src/baremetal_c_action_server/` (its `Cargo.toml`, `build.rs`, `src/main.rs` and the C
source), and `src/baremetal_c_action_server_bringup/system.toml` names it as the image's
OWNED entry (`entry = "baremetal_c_action_server"`, RFC-0071 D19), so `nros build` builds
that cargo package rather than generating an entry (phase-535 W6).

## Build

```sh
source ./activate.sh
cd examples/mps2-an385-baremetal/c/action-server && nros build
```

## Status

Builds and boots, and reaches a router on the port its `system.toml` locator
names (issue 1512 — the locator reaches the C TU as `NROS_ENTRY_LOCATOR` and
the boot config as `Config::from_image()`), and the action round trip runs
end to end under QEMU in `test_qemu_baremetal_capi_e2e` (issue 1797).
