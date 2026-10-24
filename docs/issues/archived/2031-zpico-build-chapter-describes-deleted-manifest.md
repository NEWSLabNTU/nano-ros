---
id: 2031
title: "The book's zpico-sys build chapter documents `zenoh_platforms.toml`, a file that no longer exists"
status: resolved
type: tech-debt
area: [docs, rmw-zenoh, build]
severity: low
found: 2026-10-24
resolved: 2026-10-24
related: [phase-418]
---

**Home: none.**

## What

[`book/src/internals/zpico-build.md`](../../../book/src/internals/zpico-build.md)
is written around `packages/rmw/zenoh/zpico-sys/zenoh_platforms.toml` — one
manifest with `[platform.<name>]` and `[arch.<name>]` tables, read by a single
`cc-rs` invocation. That file is gone. The data is now split by owner:

- `[port.<name>]` in `packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml` (the RMW
  descriptor, RFC-0071) holds zenoh-pico's per-port sources, defines and
  include paths; `packages/rmw/zenoh/nros-zpico-build` is the driver.
- `[arch.<name>]` profiles live in each platform's `nros-platform.toml`, gated
  by its `arch_profiles` list.

The chapter's "The manifest" section was corrected on 2026-10-24 (while
removing a stale `orin-spe` platform entry, phase-418's deferral) and now says
where the data lives, with a note pointing here. Its **"Per-platform fields"**
table, and the sections after it, still describe the old manifest's keys
(`inherits`, `defines_env`, `extra_sources`, `link.*`, `mbedtls`, …). Measured
the same day: of the keys that table lists, only `required_env` and `compile`
appear in any `nros-platform.toml`; the rest belong to `nros-rmw.toml`'s
`[port.*]` tables or to nothing.

## What would fix it

Re-derive the chapter from the two schemas that exist —
`nros-platform-config`'s `manifest.rs` and the `[port.*]` reader in
`nros-zpico-build` — rather than editing the old tables. A reader of an
`internals/` chapter is choosing where to make a change, so a table naming a
key no reader parses sends them to the wrong file.

## Resolution (2026-10-24)

The chapter was rewritten from the schemas that exist: `PortRow` and
`PlatformEntry` in `nros-platform-config/src/manifest.rs`, the platform file's
C-build keys in `platform_config.rs`, the join in
`PlatformsTree::as_platform_manifest`, and `zenoh-sources.txt`, which the old
chapter predated. It now says which file owns each key, how the `inherits`
merge treats lists, maps and scalars, and what the compile does in order.

Re-deriving it turned up three keys no reader consumed: `system_libs` (the
runner emits `pthread` itself), `exclude` (source selection moved to
`zenoh-sources.txt` in phase-420 W9) and `link` (parsed and merged, never
read). Documenting them would have repeated the defect this issue is about,
so they were deleted from `PortRow`, `PlatformEntry`, `ResolvedPlatform` and
the merge, along with the now-unused `LinkOverride`. The posix row was the
only one setting any of them; `PortRow` is `deny_unknown_fields`, so a row
that still names one fails to load rather than being silently ignored.

Also corrected: the runner's three panic messages that named
`zenoh_platforms.toml`, the `manifest.rs` module docs, and
`config/README.md`'s stale copy of the old manifest header.
