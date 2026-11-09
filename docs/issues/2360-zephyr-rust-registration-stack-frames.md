---
id: 2360
title: "A Rust Zephyr entry's registration frames are deep enough that 32 KiB of main stack does not hold an action server (measured: 63,776 B)"
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

## Measured (2026-11-09)

A Rust zenoh action server on `nucleo-f767zi-zephyr` (Renode twin, serial
zenoh), scaffolded with `nros new --lang rust --rmw zenoh --template
action-server`, with `CONFIG_INIT_STACKS` and the thread analyzer in a scratch
`os_config`:

- **At the board's 32 KiB** the image prints its entity-creation warnings, then
  nothing: no "Waiting for action goals" and no further analyzer round. That is
  issue 2359's silent overflow.
- **At 64 KiB it runs**, and the main thread peaks at **63,776 B (97 %)**. That
  is 2.25 times the talker's 28,280 B and twice the 32 KiB floor. The zenoh
  tasks are small: `zpico_exec` 1,016 B and `zpico_flush` 144 B of 16 KiB each.
- The 64 KiB image fits only with scratch changes: a 16 KiB task stack (as
  issue 2365's PR B makes the default) and `HEAP_MEM_POOL_SIZE` 16 → 12 KiB.
  RAM is then 99.41 % used. **So growing the stack is not the fix here; the
  frames are.**

The largest frames in that image (`sub sp` totals per function):

| frame | bytes |
| --- | ---: |
| `ExecutorSink::create_entity` | 26,996 |
| `ZenohSession::names_and_types_filtered` | 19,620 |
| `Executor::register_action_server_raw_sized::<1024, 1024, 1024, 4>` | 15,916 |
| `Executor::register_action_client_raw_sized::<1024, 1024, 1024>` | 13,972 |
| `CffiSession::open_marshalling_properties` | 8,420 |
| `Executor::register_service_raw_sized_inner` | 4,356 |

`create_entity` is 5,504 B in the talker and 26,996 B here: it is one `match`
over entity kinds, and its frame is sized for what LLVM inlined from every arm,
whichever one runs. Its call to `register_action_server_raw_sized` adds that
function's own 15,916 B frame on top. That function builds the arena entry's
`core` (goal, result, feedback and cancel buffers plus the pending-goal
vectors) on the stack, then `ptr::write`s an `Entry` holding it into the arena.

## Fix direction (refined)

1. Give each `create_entity` arm its own `#[inline(never)]` frame, so only the
   arm that runs is live.
2. Initialise the action server and client arena entries IN PLACE (field by
   field through the arena pointer), not as a stack temporary that is moved.
3. Re-measure the action server and client here, then set the floor from the
   measured peak.

The silent failure is the worse half, and it is the twin's, not the
board's. This board already builds with `CONFIG_HW_STACK_PROTECTION=y` and
`CONFIG_MPU_STACK_GUARD=y`, so on real hardware this overflow hits the MPU
guard and faults with a message. Renode does not enforce the MPU (issue 2359),
so the twin stays silent. `CONFIG_STACK_SENTINEL`, the software check that
would run in Renode, cannot be added here: measured, Zephyr 3.7 refuses it
beside `HW_STACK_PROTECTION` ("STACK_SENTINEL was assigned the value 'y' but
got the value 'n'"). Turning the guard off in the twin to get the sentinel
would make the twin's image differ from the board's, so this issue does not
do it. The CI-visible defence is the measured floor, refused at configure.

## Fixed: the action server and client fit 32 KiB (2026-11-09)

Three changes, each measured on the same board (`CONFIG_INIT_STACKS` plus the
thread analyzer, Renode twin, main-thread peak):

1. `ExecutorSink::create_entity` runs each `match` arm through a
   never-inlined generic (`node_runtime::out_of_line`), one instance per arm,
   so only the arm that runs has a live frame. `create_entity` itself went from
   26,996 B to 780 B.
2. The raw action server's and client's arena entries are written IN PLACE
   (`ActionServerCore::init_in_place`, `ActionClientCore::init_in_place`), not
   built as values and moved. Each core names every field with no `..` in a
   never-called function, so a field added later and not written there fails
   to compile.
3. The action server creates each of its five endpoints in its own scope, so
   its eight `heapless::String<256>` names and five infos are never all live
   at once. Moving the whole block into a separate `#[inline(never)]` function
   was tried first and measured WORSE (41,536 B): that frame stacked on top of
   the caller's.

| image (Rust, zenoh, nucleo-f767zi) | before | after |
| --- | ---: | ---: |
| talker | 28,280 B | 27,248 B |
| action server | 63,776 B | 29,184 B |
| action client | 62,232 B | 30,600 B |

"Before" was measured with a 64 KiB main stack, because at 32 KiB the action
images overflow and go silent. "After" was measured at the board's own
32 KiB: all three images run, and the analyzer keeps reporting. The floor
stays at 32 KiB, now with 1.4 to 4.7 KiB of headroom.

Left: an image that queries the graph (`ZenohSession::names_and_types_filtered`,
a 19,620 B frame) is still not measured on this board.
