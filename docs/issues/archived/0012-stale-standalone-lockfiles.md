---
id: 12
title: Stale standalone lockfiles trip the codegen ABI guard (218.J debt)
status: resolved
type: bug
area: build
related: [phase-218, phase-226]
resolved_in: abi_guard monorepo-root lock
---

Surfaced by Phase 226.F broad-build validation: `nros generate-rust` aborted
via the `nros-cli-core` `abi_guard` with `ABI version mismatch: CLI
nros-core 0.5.0 vs workspace nros-core 0.1.0`, because the Phase 218.J
`0.1.0 → 0.5.0` bump never propagated to ~56 standalone example/testing
`Cargo.lock` files. This was a false positive — the real `nros-core` source
is `0.5.0` everywhere; standalone locks aren't used for actual compilation.

Fixed: the `abi_guard` now resolves the **monorepo-root** `Cargo.lock` for
any consumer inside the nano-ros tree (detected via the
`packages/core/nros-core/Cargo.toml` marker) instead of a standalone crate's
nearest, possibly-stale lock. In-tree examples link the in-tree `nros-core`
via `[patch.crates-io]`, so the root lock (`0.5.0`) is authoritative;
external consumers keep the nearest-lock rule.
`just generate-bindings` and the broad-build preflight pass without
`NROS_SKIP_VERSION_CHECK`. (`packages/cli/nros-cli-core/src/abi_guard.rs`,
`find_monorepo_root` + `monorepo_root_lock`.) The ~56 standalone locks remain
pinned at `0.1.0` but no longer block anything; regenerating them is optional
cosmetic cleanup (the `stm32f4-porting` missing-`[workspace]` snag and the
`tests/simple-workspace` patch config remain if pursued).
