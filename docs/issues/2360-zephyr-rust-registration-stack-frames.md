---
id: 2360
title: "A Rust Zephyr entry's registration frames are deep enough that 32 KiB of main stack may not hold an action image (not measured)"
status: open
type: tech-debt
severity: medium
area: [zephyr, rust, executor]
related: [2359, phase-548]
found: 2026-11-07
---

**Home:** [phase-548](../roadmap/phase-548-three-languages-every-board.md)

Found while fixing issue 2359.

## What

A Rust nano-ros entry registers its nodes on Zephyr's main thread, and the
Rust API's frames on that path are large. Measured on `nucleo-f767zi-zephyr`
(thumbv7em-none-eabi, Renode twin): a `nros new --lang rust`
talker peaked at **28,280 bytes** of main stack (`CONFIG_INIT_STACKS`). The C
and C++ talkers on the same board fit in 16 KiB.

Issue 2359 sets the floor at 32 KiB (`NROS_RUST_MAIN_STACK_FLOOR` in
`zephyr/CMakeLists.txt`, refused at configure, and
`check-zephyr-rust-main-stack` over the board packages). That holds the
talker with 4.5 KiB to spare. It is not known to hold anything bigger.

The largest frames in that image, read from `sub sp` in its disassembly:

| frame | bytes |
| --- | ---: |
| `ZenohSession::names_and_types_filtered` | 19,584 |
| `Executor::register_action_server_raw_sized` | 15,872 |
| `Executor::register_action_client_raw_sized` | 13,952 |
| `CffiSession::open_marshalling_properties` | 8,384 |
| `ExecutorSink::create_entity` | 5,504 |
| `register_service_raw_sized_inner` | 4,352 |
| `DeclarativeNode::declare_timer_with_autostart` | 3,980 |
| `DeclarativeNode::declare_publisher_with_qos` | 3,852 |
| `rust_main` | 3,672 |

So an action server or client on the same path adds a frame of 14 to 16 KiB,
and an image that queries the graph adds 19 KiB, which 32 KiB cannot carry on
top of the talker's chain. Not measured: no Rust action image was built for
this board.

On nucleo-f767zi the main stack cannot simply grow: with
`MPU_REQUIRES_POWER_OF_TWO_ALIGNMENT` the next size is 64 KiB, and that
image overflowed RAM by 93,120 bytes.

## Fix direction

- Measure a Rust action server and a graph-query image on the board, with
  `CONFIG_INIT_STACKS`.
- Shrink the frames: the big ones hold fixed-size buffers or by-value
  structs on the stack (`open_marshalling_properties` copies 16 x 2 x 256
  bytes of `'static` property strings it could pass by pointer).
- Then lower the floor, or state it per entity kind.
