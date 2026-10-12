---
id: 1836
title: "Every NuttX Rust image dials the empty locator: the board drops the image's `system.toml` locator"
status: resolved
type: bug
area: [nuttx, rmw, testing]
severity: high
found: 2026-10-12
related: [0130, 1281, 1363, 1581, phase-484]
---

## Measured

All three NuttX arm Rust cells fail on origin/main `96a880b1c0` (a clean
worktree, fixtures built by `just nuttx build-fixtures-arm`), the same way
as on the phase-484 W4b-2 branch:

```
[ERROR] nros: [    8.941000] zpico Session -> ConnectionFailed (the two are indistinguishable downstream)
[ERROR] nros: [    8.973000] RMW session open failed — ConnectionFailed
Executor::open failed: Transport(ConnectionFailed)
```

`test_rtos_{pubsub,service,action}_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust`
— FAIL / FAIL / TIMEOUT. The C and C++ cells on the same board, kernel,
router and test pass (6/6).

The guest's own traffic, captured with QEMU's `filter-dump` on the guest
NIC (no host privileges; `tcpdump -r` reads it):

- **C `service-server`** (`tcp/10.0.2.2:8310`): ARP for 10.0.2.2 at once,
  a SYN, and the zenoh handshake completes.
- **Rust `service-server`** (`tcp/10.0.2.2:8210`): **nothing** — not an
  ARP, not a SYN — over 90 s of wall time.

## Cause

Not the network. The first suspect, `apply_ip_config`'s raw ioctls, is the
same code the passing C image runs (`configure_entry_eth0` serves both).

`arm-none-eabi-gdb` on QEMU's gdbstub, with return-value breakpoints on the
zenoh-pico open path, showed `zpico_open` → `z_open` → `_z_open` returning
`-87` ten times and never reaching `_z_open_tcp`. Inside `_z_open`,
`_z_config_get(Z_CONFIG_CONNECT_KEY)` and `(Z_CONFIG_LISTEN_KEY)` both
returned NULL, so zenoh-pico took its scouting branch and failed without a
socket. One frame up, `zpico_init_with_config` received a locator buffer of
zeros: the empty locator.

The locator string is in the image (`strings` finds `tcp/10.0.2.2:8210`, in
the `nros::main!` deploy overlay). It never reached the session because
`nros-board-nuttx::run_entry` / `run_tiers` read the connect pair from
`option_env!("NROS_LOCATOR")` / `option_env!("NROS_DOMAIN_ID")` alone and
handed them to `BootConfig::over_board_defaults`, where the BOARD's value
wins by design. Every other embedded family merges the image's overlay into
its board config first (`BaseConfig::apply_overlay`); NuttX was the one
family that did not.

That was latent while the NuttX fixture rows restated each leaf's locator as
`env = { NROS_LOCATOR, … }`. Phase-484 W4e removed those rows as a second
home, on the premise that "every board applies [the image's locator] over
the env-fed default" — a premise `check-image-locator-bake` stated in its
docstring and never checked. From then on every NuttX Rust image dialled
`""`.

## Fix

- `nros-board-nuttx-qemu` builds `Config::default()` (its `[board.net]`
  default, under a build-time `NROS_LOCATOR` / `NROS_DOMAIN_ID`), applies the
  deploy overlay with `BaseConfig::apply_overlay` (`Config::with_deploy`),
  pushes the merged ip/netmask/gateway into `eth0`, and hands the merged
  locator and domain to the family driver. One overlay serves the interface
  and the session.
- `nros-board-nuttx::run_entry` / `run_tiers` take `locator` and `domain_id`
  as arguments and read no env; its build script, whose only job was that
  env watch, is gone (the watch moved to the board crate).
- `check-image-locator-bake` verifies the claim rule 1 trusted: every board
  family's crates must apply the deploy overlay (`apply_overlay(` or a read of
  `deploy.locator`), unless `OVERLAY_ELSEWHERE` names the other road
  (Zephyr: Kconfig). A self-test runs the NuttX family driver alone as the
  negative control, and removing the board's `apply_overlay` call turns the
  gate red for `nuttx` and `nuttx-riscv`.

## Verified

Fixtures rebuilt with `just nuttx build-fixtures-arm` on the fix. The Rust
`service-server` capture now shows the ARP for 10.0.2.2, the SYN to `:8210`
and the zenoh handshake, and the guest reaches `Waiting for service
requests` at 5.5 s. `test_rtos_{pubsub,service,action}_e2e` on NuttX: 9/9
(Rust, C and C++). The rv-virt (`nuttx-riscv`) image shares the board crate
and was not run here.
