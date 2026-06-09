---
id: 1
title: Hardcoded network configuration in board crates and examples
status: resolved
type: enhancement
area: build
related: [phase-72]
resolved_in: Phase 72
---

Resolved by Phase 72: all examples now use
`Config::from_toml(include_str!("../config.toml"))` with per-example
configuration files. Users change `config.toml` and rebuild — no source code
edits needed. Board crate `Config::default()` / `Config::listener()` presets
remain for backwards compatibility but are no longer used by examples.
