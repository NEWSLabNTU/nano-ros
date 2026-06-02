# nros-board-zephyr

Phase 212.N.2 family driver crate for **Zephyr**. NetworkWait-only.

## Why "NetworkWait-only"

Zephyr is the carve-out in the 212.N Board trait family: Kconfig + DTS
already own the BSP, and the Zephyr build system emits the C `main()`
entry point. A Rust staticlib (the only shape `zephyr-lang-rust`
supports) cannot take that `main` over from Zephyr, so the usual
`<Board as BoardEntry>::run(setup)` shape does not apply.

Instead, this crate implements only [`NetworkWait`] over
`<zephyr/net/net_if.h>` — `net_if_get_default` + `net_if_is_up`,
polled every 100 ms with a 30 s budget. The other `Board` super-trait
methods (`BoardInit` / `BoardPrint` / `BoardExit`) are no-op stubs
because Zephyr already owns each of those lifecycle steps.

## How user code consumes it

From your Zephyr Rust app's `extern "C" fn rust_main()`:

```rust,ignore
use nros_board_zephyr::ZephyrBoard;
use nros_platform::board::NetworkWait;

#[no_mangle]
pub extern "C" fn rust_main() {
    if ZephyrBoard::wait_link_up().is_err() {
        // log + bail
        return;
    }
    // ... open RMW session, run executor ...
}
```

## Build limitations

`cargo check --offline` on the host succeeds — the Zephyr C symbols
(`net_if_get_default`, `net_if_is_up`, `k_msleep`) are declared via
`extern "C"` and have no host link.

`cargo build` on a vanilla host will fail to link those symbols.
That is expected: the crate is consumed as a Rust staticlib by
Zephyr's `rust_cargo_application()` cmake function, which provides
the Zephyr kernel symbols at link time.

This crate sits **outside** the nano-ros Cargo workspace (empty
`[workspace]` stub in `Cargo.toml`).
