---
id: 2656
title: "`packages/cli` inherits the root config's dev profile, `incremental = false`, so its `cargo test` rebuilds take 32 s"
status: open
type: perf
area: [cli, build, testing]
severity: low
found: 2026-11-16
related: [phase-489]
---

**Home: [phase-489](../roadmap/phase-489-ci-build-speedup.md)** (build speed). Measured 2026-11-16 on host 118 (48 cores, load 11–36 from other sessions), `packages/cli` at main `c49aaaaef0`, each build in its own target dir. Raw logs and `--timings` reports: `cli-profile/` in the measuring worktree.

## Measured

The repo-root `.cargo/config.toml` sets `[profile.dev] opt-level = 1,
incremental = false` (phase-340 W1, for the fixture leaves). Cargo's config
walk-up applies it to `packages/cli` too, where it was not measured:

| build | wall | `nros-cli-core` |
| --- | --- | --- |
| dev, cold | 42.3 s | 30.3 s |
| dev, edit one `nros-cli-core` source | **31.7 s** | 29.9 s (frontend 12.6, codegen 17.3) |

The release + incremental edit rebuild is 4.7 s (issue 2654), so the dev build
is about 7x slower than it needs to be. It is what a contributor's
`cargo test -p nros-cli-core` and the `cli-tests` gate (`cargo test --manifest-path
packages/cli/Cargo.toml --workspace`) build with.

## Opportunity and cost

`packages/cli/.cargo/config.toml` (or `[profile.dev]` in `packages/cli/Cargo.toml`;
check which wins over the walked-up file) with `incremental = true`. Estimated
edit rebuild about 4–5 s. Low cost. The same sccache caveat as issue 2654
applies to CI. A debug binary is 425 MB against 39 MB release, so dev's
`debug = true` adds link time, measured but not separated here.

## Ruling (2026-11-16, user): HELD

`dev` stays `incremental = false`, with the reason stated beside it in the
shared config. Phase-340 W6 (78375a2d5d) found that incremental on `dev` made
the Zephyr west lane's rlibs differ from build to build, and sccache keys on
rlib CONTENT: with it off, 115 of 143 became byte-identical. Turning it back on
for the CLI's sake would undo that. This stays open as the measured cost of
that trade-off, not as work.
