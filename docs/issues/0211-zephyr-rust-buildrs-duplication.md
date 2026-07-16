---
id: 211
title: "Kconfig→rustc-env locator-bake build.rs copy-pasted into every zephyr rust example and workspace zephyr_entry"
status: open
type: tech-debt
area: examples
related: []
---

## Problem (audit 2026-07-16, J1/I1)

`examples/zephyr/rust/*/build.rs` and every workspace `zephyr_entry`
`build.rs` (`ws-lifecycle`/`ws-params`/`ws-qos`/`ws-realtime`/`ws-safety`
rust variants, `workspaces/rust`) carry a near-identical file: the
Kconfig→cfg bridge plus the CONFIG_NROS_ZENOH_LOCATOR / DOMAIN_ID / XRCE
agent rustc-env bake (the documented known-issue #17 empty-locator
workaround). Copy-out users inherit ~50 lines of build plumbing, and a fix
to the workaround means N edits.

## Fix sketch

A small `nros-zephyr-build` helper crate (build-dependency) owning kconfig
export + locator baking; each example's build.rs collapses to one call.
