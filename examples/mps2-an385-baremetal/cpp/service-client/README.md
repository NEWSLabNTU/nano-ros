# Bare-metal C++ service client (MPS2-AN385, no RTOS)

Calls `example_interfaces/AddTwoInts` on `/add_two_ints` once (2 + 3) and logs the sum, from an ordinary nano-ros **C++** application on a Cortex-M3 with **no
RTOS at all** — the same program as `examples/mps2-an385-freertos/cpp/service-client/`.

Same shape as [`../talker`](../talker/README.md), whose README explains it: the
link root is Rust (`src/main.rs` boots the board and calls `app_main()`),
`build.rs` compiles the C++ source into it and generates the message bindings,
and there is no `printf`, so log lines are built by hand and emitted through
`nros_log_emit_at` (issue 1512).

Build with the fixture lane (`just build-test-fixtures`) or, from this
directory, `nros sync` then `cargo build --release`.
