---
id: 2299
title: "Every Rust host-tool build warns \"patch … was not used\" three times per cargo call, because sync's central patch set assumes every consumer names `nros`"
status: open
type: bug
area: [cli, build]
severity: low
found: 2026-11-06
related: [phase-527, phase-287, 0457]
---

## What happens

`nros configure` writes a `.cargo/config.toml` into every `nros_cargo` package. Its
one `include` names the central `<checkout>/nros-patch.toml`, which patches
`nros`, `nros-core` and `nros-serdes` (`cmd/ws.rs` `CENTRAL_PATCH_CRATES`).
A runner or command package (phase-527, RFC-0119) is a host tool written
against `nros-extension`. It names none of the three. So every cargo call that
builds it prints:

    warning: patch `nros v…` was not used in the crate graph

once per crate, three per call. The phase-527 W7 e2e run printed 12 such lines.
None of them names `nros-extension`: `builder/host_tool.rs` already adds that row
only for a package that depends on it.

## Why

`CENTRAL_PATCH_CRATES` is admitted by the rule in its doc comment: "a crate may
live centrally only if it is registry-named in EVERY Rust consumer's dependency
graph". That held while every `nros_cargo` package was an image or a node. A
host package breaks it, and the rule had no way to see that, because the central
file is shared by construction.

## Direction

Don't write the central `include` into a host package's config. A host package
is identified by `nros-runner.toml` / `nros-command.toml`, the same test
`scripts/stage-sdk-root.sh` uses. Or narrow the row set by what the package's
own graph names registry-style, as `registry_patches` does for images. Fix the
class rather than the site: list every writer of a sync-managed
`.cargo/config.toml` and check each against a host package.

Acceptance: `nros build` of a scaffolded runner package (`nros new runner`)
prints no "was not used in the crate graph" line, and an image in the same
workspace still resolves `nros` through the central patch.
