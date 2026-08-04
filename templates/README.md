# nros Embedded Templates

Cargo.toml templates for host-ecosystem projects that consume nano-ros.

## Available Templates

| Template            | Target | Framework   | Description             |
|---------------------|--------|-------------|-------------------------|
| `cargo-zephyr.toml` | Zephyr | Zephyr RTOS | For west-based projects |

The three STM32F4 templates (`cargo-{rtic,embassy,polling}-stm32f4.toml`) left
with their board crates in phase-337 W7.a. The board is now a worked
out-of-tree example — [`book/src/porting/stm32f4-out-of-tree.md`][oot] carries
the descriptor, the memory map and the `Config` shape, which is more than the
templates did: a template is a file to copy, not a path through the
customization ladder.

[oot]: ../book/src/porting/stm32f4-out-of-tree.md

## Usage

1. Copy the appropriate template to your project as `Cargo.toml`
2. Update the `[package]` section with your project name
3. Adjust paths in `[dependencies]` for nros crates
4. Copy the corresponding `.cargo/config.toml` from `examples/`

## Feature Flags

Templates use these nros features:

- `default-features = false` — disables std/alloc
- `features = ["sync-critical-section"]` — uses critical-section for sync
