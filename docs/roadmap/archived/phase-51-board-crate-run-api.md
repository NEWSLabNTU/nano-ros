# Phase 51: Board Crate `run()` API

**Status**: Complete

## Background

Board crates (`nros-board-mps2-an385`, `nros-board-stm32f4`, `nros-board-esp32`, `nros-board-esp32-qemu`) currently
expose a simplified `Node` wrapper that only supports pub/sub via `create_publisher()` and
`create_subscription()`. This blocks users from using services, actions, timers, callbacks,
async, and the full Executor API on embedded targets.

The root cause is that `run_node()` opens the RMW session internally and wraps it in a
simplified `Node` type, hiding the `Executor` from the user.

## Design

Replace `run_node()` with `run()`:

```rust
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
```

**What `run()` does** (hardware init — steps 1–5 from current `run_node()`):
1. Hardware init (clocks, GPIO, Ethernet/WiFi drivers, cycle counter)
2. smoltcp interface + socket set creation
3. IP configuration (static or DHCP)
4. `SmoltcpBridge::init()` + socket registration + poll callback setup
5. Call user closure with `&Config`
6. Handle success/error exit

**What `run()` does NOT do** (removed — user does this via Executor):
- `RmwConfig` / `ZenohRmw::open()` / session creation
- Simplified `Node` wrapper creation
- `node.shutdown()` / `clear_network_state()`

The user's closure calls `Executor::open()` directly, getting full API access:

```rust
run(Config::default(), |config| {
    let exec_config = ExecutorConfig::new(config.zenoh_locator)
        .domain_id(config.domain_id);
    let mut executor = Executor::<_, 0, 0>::open(&exec_config)?;
    let mut node = executor.create_node("talker")?;
    let publisher = node.create_publisher::<Int32>("/chatter")?;
    // Full Executor API available: services, actions, timers, callbacks, async...
    Ok(())
})
```

## Work Items

- [x] 51.1 — `nros-board-mps2-an385`: Add `run()`, remove `run_node()`, remove simplified `Node`
- [x] 51.2 — `nros-board-stm32f4`: Add `run()`, remove `run_node()`, remove simplified `Node`
- [x] 51.3 — `nros-board-esp32`: Add `run()`, remove `run_node()`, remove simplified `Node`
- [x] 51.4 — `nros-board-esp32-qemu`: Add `run()`, remove `run_node()`, remove simplified `Node`
- [x] 51.5 — Migrate QEMU ARM examples (`talker`, `listener`, `large-msg-test`) to `run()`
- [x] 51.6 — Migrate STM32F4 example (`talker`) to `run()`
- [x] 51.7 — Migrate ESP32 examples (`talker`, `listener`) to `run()`
- [x] 51.8 — Migrate QEMU ESP32 examples (`talker`, `listener`) to `run()`
- [x] 51.9 — Verify QEMU ARM integration tests pass (`just test-qemu`)
- [x] 51.10 — Clean up: remove `Publisher`, `Subscription`, `Node` wrapper types from board crates
- [x] 51.11 — Update CLAUDE.md board crate description

## Dependency Changes

Since `run()` doesn't open a session, board crates no longer need `nros-rmw`, `nros-rmw-zenoh`,
or `nros-core`. Examples now depend on `nros` directly (for `Executor`, `ExecutorConfig`, etc.).

Board crate remaining deps:
- Hardware driver (lan9118-smoltcp / stm32-eth / openeth-smoltcp / esp-radio)
- smoltcp (interface, sockets)
- zpico-smoltcp (bridge init, socket registration, poll callback)
- Platform crate (clock, RNG, network state globals)

Board crate `ros-humble`/`ros-iron` features are removed (no longer forwarded to RMW).
Examples add these features on the `nros` dependency instead.

## Acceptance Criteria

- `run_node()` deleted from all 4 board crates — only `run()` in public API
- Simplified `Node`, `Publisher`, `Subscription` types removed from board crates
- All 7 existing embedded examples migrated to `run()` + `Executor::open()` pattern
- QEMU ARM examples build and pass existing integration tests (`just test-qemu`)
- Board crate re-exports only: `Config`, `run`, platform utilities, `exit_success`/`exit_failure`
- `just quality` passes

## Notes

- The `-> !` return type on `run()` is essential — stack-allocated hardware objects (Interface,
  SocketSet, EthernetDMA) have raw pointers stored in global statics via `set_network_state()`.
  Never returning keeps these objects alive.
- `run()` uses a generic error type `E: core::fmt::Debug` so users can return their own errors
  (e.g., `nros::NodeError`). No board-specific error type needed for `run()`.
- ESP32-QEMU's retry logic for `ZenohRmw::open()` was in `run_node()`. Users of `run()` can
  implement their own retry logic if needed, or just call `Executor::open()` directly.
