# Bare-metal C++ action client (MPS2-AN385, no RTOS)

Sends one `example_interfaces/Fibonacci` goal (order 10) to `/fibonacci` and logs feedback and the result, from an ordinary nano-ros **C++** application on a Cortex-M3 with **no
RTOS at all** — the same program as `examples/mps2-an385-freertos/cpp/action-client/`.

Same shape as [`../talker`](../talker/README.md), whose README explains it: the
link root is Rust (`src/main.rs` boots the board and calls `app_main()`),
`build.rs` compiles the C++ source into it and generates the message bindings,
and there is no `printf`, so log lines are built by hand and emitted through
`nros_log_emit_at` (issue 1512).

Build with the fixture lane (`just build-test-fixtures`) or, from this
directory, `nros sync` then `cargo build --release`.
