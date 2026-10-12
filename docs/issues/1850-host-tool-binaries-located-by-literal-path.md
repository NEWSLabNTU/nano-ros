---
id: 1850
title: "The in-tree `nros` and `nros-launch-resolve` binaries are located by literal path at ~50 sites, beside the resolvers that exist for them"
status: open
type: tech-debt
area: build
severity: low
found: 2026-10-13
related: [0285, 0409, 1280, 1454, 1641, 1767, RFC-0103, phase-484]
---

## Measured (2026-10-13, tree at `a310e03054` + the 1767 close-out)

Residue of issue 1767 items 8 and 9: two first-party host tools, each with a
resolver, each still spelled as a literal path by most of its shell consumers.

**`nros`.** The shell resolver is `nros_cli_bin` (`scripts/build/cargo.sh:454`:
`$NROS_CLI` → this checkout's `packages/cli/target/release/nros` → PATH →
`$NROS_HOME/bin/nros`); cmake's is `nros_resolve_cli`
(`cmake/NanoRosCli.cmake:36`: `$NROS_CLI` → cached tool var → PATH → store).
Outside `cargo.sh`, the literal `packages/cli/target/release/nros` appears on
49 non-comment lines in 23 files under `just/`, `justfile` and `scripts/`
(`grep -rn "packages/cli/target/release/nros\b" just justfile scripts | grep -v
nros-launch | grep -v ':\s*#'`), the largest being `justfile` (11),
`just/check/cmake.just` (6: `:336`, `:346`, `:393`, `:403`, `:423`, `:432`),
`just/ros-editions.just` (3), `scripts/check-template-copy-out.sh` (3). Some
are `${NROS_CLI:-<literal>}` (`justfile:634`, `just/check/fixtures.just:235`,
`just/check/rmw.just:101`, `just/check/tools.just:228`), i.e. rungs 1–2 of
`nros_cli_bin` restated without rungs 3–4; most are the bare literal.

**`nros-launch-resolve`.** Three ladders:

- `nros_orchestration_ir::model_location::launch_resolver_bin_near`
  (`packages/core/nros-orchestration-ir/src/model_location.rs:333`):
  `$NROS_LAUNCH_RESOLVE` → the checkout owning `near` (or this crate's) →
  `<store>/bin/nros-launch-resolve`.
- `nros-cli-core` `cmd/ws.rs:2614` `launch_resolver_path` / `resolver_from`:
  `$NROS_LAUNCH_RESOLVE` → sibling of the running `nros` → the running CLI's
  checkout → (own SDK root).
- `scripts/build/launch-resolver-identity.sh:59`: `$NROS_LAUNCH_RESOLVE` →
  `${CARGO_TARGET_DIR:-<crate>/target}/release` → `packages/cli/target/release`;
  no store rung, no sibling rung. Plus the literals at `justfile:896`
  and `justfile:4391`.

## Why it is not one-line work

Most `nros` sites are PRECONDITION probes of a gate ("is this checkout's CLI
built?"), and `nros_cli_bin` deliberately falls to PATH and the store — so
routing them through it unchanged would turn a correct skip into a run against
another tree's binary (issue 1280's class). The fix is a mode on the ONE
resolver (checkout-only), not a second function, then a sweep with a gate
holding the literal out of code (negative control: a re-added literal).

The launch resolver's three ladders want one owner — most naturally `nros
locate tool nros-launch-resolve` (RFC-0103 D4's `nros locate`, extended to
`[tool.*]`-like first-party binaries) read by the shell, with the two Rust
ladders folded onto `launch_resolver_bin_near`.

## Direction

RFC-0103 D4 (one resolver, every road reads its answer).
