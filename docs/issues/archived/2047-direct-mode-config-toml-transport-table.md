---
id: 2047
title: "Direct-mode `config.toml` still carries RFC-0004's `[[transport]]` (board `Config::from_toml`)"
status: resolved
type: tech-debt
area: [boards, config]
severity: low
found: 2026-10-25
related: [phase-514, RFC-0109, RFC-0004]
resolved: 2026-10-27
---

## What

RFC-0109 D6 retires `[[transport]]` (RFC-0004 §6). Phase-514 W4 refuses it in
`system.toml`, but a second home for it survives: the hand-written
direct-mode `config.toml` that seven board crates parse with
`Config::from_toml(include_str!(…))` — `nros-board-common::BaseConfig::apply_toml_field`
plus each board's `Config::from_toml` (`esp32-qemu`, `freertos`,
`mps2-an385`, `nuttx-qemu`, `threadx-linux`, `rv-virt-threadx`).

Tracked users (`git grep -l '^\[\[transport\]\]'`):

- `packages/testing/nros-tests/bins/{condvar-contract-freertos-mps2,logging-smoke-esp32-qemu,logging-smoke-freertos-mps2,logging-smoke-threadx-riscv64}/config.toml`;
- `book/src/user-guide/configuration.md` (the `config.toml` row and § "Direct-mode");
- `packages/cli/cargo-nano-ros/src/scaffold.rs` (scaffolds one).

## Why it was left

It is a board-crate API for apps with no `system.toml` and no `nros::main!`,
not the deployment schema W4 retired, and on those boards nano-ros IS the OS,
so the board package legitimately owns the address (RFC-0109 D4). Retiring
it means giving those apps the D4 board network file instead (`[eth0]`
`network`/`host`/`mac` plus `[network.<name>]`), parsed `no_std` at
compile time, and moving `locator` / `domain_id` to whatever direct-mode
apps use for the default domain. That is a public API change in seven
crates, so it is its own piece of work.

## Done when

`[[transport]]` appears in no tracked file, `Config::from_toml` reads the D4
format (or is removed for a builder the four bins call), and the book's
direct-mode section shows the new shape.

## Resolution

The direct-mode `config.toml` is now the RFC-0109 D4 board network file, plus
a `[node]` table for what a `system.toml` session would carry:

```toml
[node]
domain_id = 0
locator = "tcp/10.0.2.2:7451"

[eth0]
network = "qemu"      # built in: 10.0.2.0/24, gateway 10.0.2.2
host = 98             # or: address = "10.0.2.98"
mac = "02:00:00:00:00:98"
```

One `no_std` parser reads it, `nros_board_common::parse_direct_toml`, returning
a `DirectConfig` of what the file states; the six boards' `Config::from_toml`
(`esp32-qemu`, `freertos`, `mps2-an385`, `nuttx-qemu`, `threadx-linux`,
`threadx-qemu-riscv64`) apply it and keep only their own keys —
`[node.rt*]` on FreeRTOS, a serial interface's `baud`, ThreadX Linux's host
veth as `[eth0] device`. Four of the six had their own copy of the scanner and
of the IPv4 / MAC / integer parsers; those copies are gone.
`BaseConfig::apply_toml_field` is replaced by `BaseConfig::apply_direct`.

A malformed value still keeps the board default (firmware has nowhere to
report it), but a wrong shape now panics at boot naming the fix instead of
booting on defaults the author believes they replaced: `[[transport]]`, a
second addressed interface, `host` with no `network`, an undefined network, a
`host` outside its network, and `address = "dhcp"`.

The four `nros-tests` bins, the `cargo nano-ros` scaffold and the book's
direct-mode section use the new shape; `[[transport]]` remains only in
archived phase docs and in the refusals that name it.
