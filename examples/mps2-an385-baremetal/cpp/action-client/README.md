# Bare-metal C++ action client (MPS2-AN385, no RTOS)

Sends one `example_interfaces/Fibonacci` goal (order 10) to `/fibonacci` and logs feedback and the result, from an ordinary nano-ros **C++** application on a Cortex-M3 with **no
RTOS at all** — the same program as `examples/workspaces/cpp/src/action_client_pkg/`.

Same shape as [`../talker`](../talker/README.md), whose README explains it: the
link root is Rust (`src/main.rs` boots the board and calls `nros_app_main()`),
`build.rs` compiles the C++ source into it and generates the message bindings,
and there is no `printf`, so log lines are built by hand and emitted through
`rclc_log_emit_at` (issue 1512).

This directory is a one-package workspace (RFC-0122 D3): the program is
`src/baremetal_cpp_action_client/` (its `Cargo.toml`, `build.rs`, `src/main.rs` and the C++
source), and `src/baremetal_cpp_action_client_bringup/system.toml` names it as the image's
OWNED entry (`entry = "baremetal_cpp_action_client"`, RFC-0071 D19), so `nros build` builds
that cargo package rather than generating an entry (phase-535 W6).

Build with the fixture lane (`just build-test-fixtures`) or, from this
directory, `nros build`.
