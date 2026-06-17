---
id: 81
title: Remove board-crate `Config::from_toml` — dead legacy, superseded by DeployOverlay
status: open
type: tech-debt
area: boards
related: [phase-256, rfc-0004]
---

## Decision (2026-06-18)

The board crates' `Config::from_toml(include_str!("config.toml"))` parsers are
**dead legacy** and are removed — but in their own embedded sweep, **separate from
the phase-256 config tidy** (which is orchestration-scoped). Split out so phase-256
W9 stays focused and this board-crate sweep gets its own review.

## Why dead

`config.toml` is retired (RFC-0004 §8) and **0 examples ship one**. The embedded
board config that `from_toml` was meant to parse now comes from
`[package.metadata.nros.deploy.<t>]` → `DeployOverlay`, baked by `nros::main!()` and
applied via `BoardEntry::run_with_deploy` onto the board boot `Config`. E.g.
`examples/stm32f4/rust/talker` declares `locator`/`ip`/`gateway`/`netmask` in deploy
metadata, not a `config.toml`. So `from_toml` is never reached.

## Scope — what comes out

- `Config::from_toml` (and any `include_str!("config.toml")` / `include_str!("nros.toml")`
  call sites) in the board crates: `nros-board-{stm32f4,rtic-stm32f4,nuttx-qemu-arm,
  nuttx-qemu-riscv,esp32-qemu,mps2-an385,threadx-qemu-riscv64,freertos,...}/src/config.rs`
  + `nros-board-common/src/board_init.rs` doc references + `nros-board-cffi/include/nros/board.h`.
- The CLAUDE.md pitfall-index line referencing `Config::from_toml(include_str!("config.toml"))`.

## What STAYS

- The board `Config` struct + its hardcoded default constructors (`nucleo_f429zi()` etc.)
  and `Config::default()` — the live boot config.
- `DeployOverlay` + `BoardEntry::run_with_deploy` — the live embedded-config path.

## Notes

- Pure deletion of an unreachable code path; no example/fixture exercises `from_toml`.
- Do after (or alongside) phase-256 W9 ① (the orchestration `nros.toml` overlay removal),
  so all `config.toml`/`nros.toml` file surfaces retire together but in reviewable chunks.
