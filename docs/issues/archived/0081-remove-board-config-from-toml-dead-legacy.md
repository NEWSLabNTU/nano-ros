---
id: 81
title: Remove board-crate `Config::from_toml` — dead legacy, superseded by DeployOverlay
status: wontfix
type: tech-debt
area: boards
related: [phase-256, rfc-0004]
---

## Resolution: WONTFIX — keep `from_toml` (2026-06-18)

**Premise was wrong + conflicts with the maintainer's config principle.** Two findings
reversed the delete decision:

1. **Not 0-user dead.** `Config::from_toml` has **3 live callers** — the
   `logging-smoke-{esp32-qemu,freertos-mps2,threadx-riscv64}` fixtures. They had inlined
   the config as a `const &str` in `main.rs`; removal would have forced the network
   params (`ip`/`mac`/`gateway`/`locator`) to be **hardcoded in Rust code** (builder calls).
2. **Maintainer principle: config belongs in files, not hardcoded in code.** A standalone
   compile-baked `config.toml` (read via `include_str!` → `from_toml`) is a **supported
   first-class file path** for hand-written `no_std` embedded apps that bypass the
   `nros::main!()` / `DeployOverlay` codegen pipeline. `from_toml` is the mechanism for it.

**Action taken instead of deletion:** moved the 3 fixtures' inline `const CONFIG` into
sibling `config.toml` files (`include_str!("../config.toml")`), so the board net config
lives in a file, not code — and Path B now has real file users (no longer 0).

**Config-in-files homes (both supported):** `[package.metadata.nros.deploy.<t>]` →
`DeployOverlay` (codegen apps) **and** standalone `config.toml` → `from_toml` (hand-written
embedded apps/fixtures). **Code home:** board `Config::default().with_*()` builders.

**RFC-0004 reconciliation (follow-up):** RFC-0004 §8 currently calls `config.toml` "retired"
and claims 0 files. With this decision `config.toml`/`from_toml` is a kept, if niche,
standalone-file path — RFC-0004 §5/§8 should be updated to record it as supported (not
retired). Tracked here until the RFC edit lands.

## Original (superseded) Decision (2026-06-18)

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
