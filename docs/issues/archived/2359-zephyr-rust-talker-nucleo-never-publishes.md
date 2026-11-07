---
id: 2359
title: "A Rust talker on nucleo-f767zi-zephyr over the serial link boots and never runs its timer callback; C and C++ on the same board and link publish"
status: resolved
type: bug
severity: high
area: [zephyr, rust, zenoh, serial, executor]
related: [phase-548, 2358, 2164]
found: 2026-11-07
resolved: 2026-11-07
resolved_in: "zephyr/CMakeLists.txt refuses a Rust image whose CONFIG_MAIN_STACK_SIZE is below NROS_RUST_MAIN_STACK_FLOOR (32768); the nucleo-f767zi, rpi-pico-w and fvp-aemv8r-smp confs move to 32768"
---

**Home: [phase-548](../../roadmap/phase-548-three-languages-every-board.md)**
(found by W7; it is the Rust cell's remaining red on `nucleo-f767zi-zephyr`).

## What

Measured in the `ros2` box (Humble, Zephyr 3.7, Renode 1.17.0), in a west
workspace written as the Nucleo starter writes it, plus the
`zephyr-lang-rust` project issue 2358 is about, and with the board's
`rust_target` corrected to `thumbv7em-none-eabi` (fixed in the same PR as
this filing).

```bash
nros new nucleo_rs --board nucleo-f767zi-zephyr --template talker --lang rust --rmw zenoh
cd nucleo_rs
nros run nucleo_rs_node --expect 'Publishing:'
```

The image builds, Renode boots it, and `nros run` starts the router on the
twin's USART3 pty. The console then prints, and nothing after it in 461 s:

```text
*** Booting Zephyr OS build 36940db938a8 ***
[INFO] rustapp: [    0.023528] nros: zephyr workspace entry up (1 nodes)
[INFO] nros: [    0.023546] arena over-provisioned: ... 184/14424 bytes claimed at first spin; ...
```

What the measurement rules out:

- **Not Rust on Zephyr in general.** The same template on
  `mps2-an385-zephyr` (QEMU, TCP to a router on `0.0.0.0:7447`) prints
  `Publishing: 'Hello World: 1'` 1 s after `entry up`.
- **Not the board, the link or the router.** The C and the C++ talker on
  the same board, the same west workspace and the same serial link publish
  within 9 s, and `ros2 topic echo /chatter` receives them.
- **Not the entry's park budget.** With `const HAS_TICK: bool = true`, so
  the entry spins with the 10 ms `ENTRY_POLL_BUDGET` instead of
  `ENTRY_IDLE_BUDGET`, nothing changes in 180 s.
- **The timer callback never runs.** A `log_info!` added at the top of
  `on_callback`, before the publish, never prints in 146 s (the string is in
  the ELF). So the publish does not block: the dispatch never happens. Most
  likely the main thread never returns from its first `spin_once`.
- Kconfig: `CONFIG_MAIN_THREAD_PRIORITY=5`, `CONFIG_NUM_PREEMPT_PRIORITIES=15`,
  `CONFIG_NROS_ZENOH_READ_PRIORITY=200`, the same in the C and the Rust
  image. The two `.config` files differ only in the language symbols,
  `CONFIG_NROS_ZEPHYR_HEAP_SIZE` (65536 for C, 131072 for Rust) and
  `POSIX_SYSCONF_IMPL_FULL` against `_MACRO`.

So the fault sits in what only the Rust entry does on a serial zenoh
session: the generated `nros::main!` Zephyr loop
(`packages/core/nros-macros/src/main_macro.rs`, the `loop { spin_once(budget) }`
after `entry up`), the Rust executor's transport drain or park on a serial
session, or the Rust timer's clock on this target.

The run also showed: `nros run` does not stop when the image stops making
progress, which is expected without `--expect` matching, and a Zephyr image
that panics (seen on mps2 with no router) leaves QEMU running until the
caller's timeout. The second is phase-548 W4's class on Zephyr.

## Fix direction

- Find where the main thread sits: Renode's monitor (`sysbus.cpu PC`, or a
  GDB stub on the twin) after `entry up`, or `CONFIG_THREAD_ANALYZER` and a
  console line per `spin_once` return.
- Compare the Rust and the C entry's spin on a serial session: what the C
  entry passes to `spin_once`, and whether the Rust path blocks in the
  zenoh-pico serial read or in a wake wait that the timer deadline does not
  bound.
- Then fix the class for every serial Rust Zephyr image, not this board
  only.

## Done when

- The talker above prints `Publishing: 'Hello World: 1'` under
  `nros run nucleo_rs_node --expect 'Publishing:'` in the Renode twin, and
  `ros2 topic echo /chatter` receives it.
- The phase-548 table's nucleo-f767zi Rust cell reads ok.

## Resolution (2026-11-07)

**Cause:** the main thread overflows its 16 KiB stack while it registers the
node (peak measured at 28,280 B). The overflow zeroes the idle thread's frame,
and idle then faults with `K_ERR_ARM_USAGE_ILLEGAL_EPSR`. Renode does not
enforce the MPU stack guard, so nothing prints. `mps2-an385-zephyr` works only
because its main stack is 128 KiB.

**Fix:** `zephyr/CMakeLists.txt` refuses a Rust image whose
`CONFIG_MAIN_STACK_SIZE` is below `NROS_RUST_MAIN_STACK_FLOOR` = 32768
(native_sim is exempt). The prj.conf of nucleo-f767zi, rpi-pico-w and
fvp-aemv8r-smp moves to 32768, and the new gate `just check zephyr-rust-main-stack`
holds the floor. Measured: Rust and C on the Nucleo, and Rust on mps2, all
publish, and `ros2 topic echo` receives. Not measured: the rpi-pico-w's RAM fit
at 32 KiB. Whether action and graph frames fit in 32 KiB is issue 2360.
