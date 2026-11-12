# Bare-metal C++ service server (MPS2-AN385, no RTOS)

Serves `example_interfaces/AddTwoInts` on `/add_two_ints`, from an ordinary nano-ros **C++** application on a Cortex-M3 with **no
RTOS at all** — the same program as `examples/workspaces/cpp/src/service_server_pkg/`.

Same shape as [`../talker`](../talker/README.md), whose README explains it: the
link root is Rust (`src/main.rs` boots the board and calls `nros_app_main()`),
`build.rs` compiles the C++ source into it and generates the message bindings,
and there is no `printf`, so log lines are built by hand and emitted through
`rclc_log_emit_at` (issue 1512).

This directory is a one-package workspace (RFC-0122 D3): the program is
`src/baremetal_cpp_service_server/` (its `Cargo.toml`, `build.rs`, `src/main.rs` and the C++
source), and `src/baremetal_cpp_service_server_bringup/system.toml` names it as the image's
OWNED entry (`entry = "baremetal_cpp_service_server"`, RFC-0071 D19), so `nros build` builds
that cargo package rather than generating an entry (phase-535 W6).

Build with the fixture lane (`just build-test-fixtures`) or, from this
directory, `nros build`.
