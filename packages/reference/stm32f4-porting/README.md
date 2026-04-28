# STM32F4 Porting References

These are **porting reference implementations**, not user-facing examples. They demonstrate low-level hardware integration patterns for STM32F4 boards using internal platform crates (`nros-rmw-zenoh`, `zpico-smoltcp`).

## Contents

| Directory  | Description                                                |
|------------|------------------------------------------------------------|
| `polling/` | Bare-metal polling loop with smoltcp + zenoh-pico          |
| `rtic/`    | RTIC task-based execution with interrupt-driven networking |

## Why not in `examples/`?

These reference implementations use internal platform crates directly (`nros-rmw-zenoh`, `zpico-smoltcp`) rather than a board support crate. They serve as templates for BSP developers creating new board support crates, not as end-user examples.

For user-facing STM32F4 examples, see:
- `examples/stm32f4/rust/zenoh/talker/` - Publisher using the `nros-board-stm32f4` board crate
