---
id: 66
title: CI red on main — stale example Cargo.locks (nros-core 0.1.0) trip the ABI guard + clippy empty-line-after-doc-comment in nros/lib.rs
status: resolved
type: bug
area: build
related: [phase-244, issue-0057, issue-0062]
resolved_in: 1193e26c7
---

> **RENUMBERED 64 → 66 (2026-06-15).** A concurrent agent independently created a
> different issue 0064 (esp32-c3 QEMU load-access-fault, still open). This resolved
> CI-infra issue (created first) was moved to the next free id 66 to break the
> collision; the open esp32 issue keeps 0064 to stay consistent with its active
> commit refs. NOTE: this issue's own merged commits (`1193e26c7`, `8999a4520`,
> `cf0e69485`) say "#64"/"#0064" — those historical refs mean THIS issue (now 0066).
>
> **RESOLVED (2026-06-15, `1193e26c7`).** A: regenerated the 10 example Cargo.locks
> pinning `nros-core 0.1.0` → `0.5.0` (6 qemu-arm-nuttx/rust + 4 stm32f4/rust *-rtic)
> — validated by clean `just nuttx build-examples` + `stm32f4 build-fixtures`
> (abi_guard no longer aborts). B: moved the Phase-248 C7 `//` note above the `///`
> doc block in `nros/lib.rs` — `check-workspace-all` clippy clean. (The sibling
> `check`-cell reds were split to [[0065-check-cell-red-stale-nros-feature-combo-and-clang-format]].)

## Symptom

Two CI checks are RED on `main` (and therefore on every PR), unrelated to the
PR's source. Surfaced while gating the phase-244 PR #2
(`phase-244-native-shape-b`), whose diff is disjoint from both failing trees
(only `docs/`, `examples/native/rust/*/src/main.rs`,
`examples/zephyr/cpp/cyclonedds/talker-aemv8r/*`).

### A — `platform` cell: ABI guard abort on a stale example lock (PRIMARY)

`nros generate-rust` for `examples/qemu-arm-nuttx/rust/action-client` aborts:

```
Error: nros generate-rust aborted: ABI version mismatch between the `nros` CLI
binary and the runtime `nros-core` your workspace resolves.
  CLI binary nros-core version: 0.5.0
  Workspace Cargo.lock at:      Cargo.lock
  Workspace nros-core version:  0.1.0
  (nros-cli-core/src/abi_guard.rs:218)
```

Run: `platform` job in PR #2 CI (`actions/runs/27507282190`).

### B — `check` cell: clippy `empty-line-after-doc-comments` (SECONDARY, found while investigating)

```
error: empty line after doc comment
  --> packages/core/nros/src/lib.rs:382:1
  = note: `-D clippy::empty-line-after-doc-comments` implied by `-D warnings`
```

Run: `check` job — RED on `main` itself (`actions/runs/27505977548`, commit
`feat(249 P3)`), not just on PRs.

## Root cause

**A.** The root workspace is `version = "0.5.0"` and the root `Cargo.lock`
correctly resolves `nros-core 0.5.0`. But several **committed example
Cargo.locks still pin `nros-core 0.1.0`** (stale — predate the 0.x→0.5.0 bump):

```
examples/qemu-arm-nuttx/rust/action-client/Cargo.lock   # nros-core 0.1.0
examples/qemu-arm-nuttx/rust/listener/Cargo.lock        # nros-core 0.1.0
examples/qemu-arm-nuttx/rust/service-client/Cargo.lock  # nros-core 0.1.0
# (sweep the example tree — likely more)
```

`abi_guard` prefers the monorepo-root lock for in-tree consumers
(`monorepo_root_lock(start)`), but in the platform cell's copy-out / per-example
build context the monorepo marker is not above `start`, so it falls back to the
example's own (stale `0.1.0`) lock → `versions_match("0.5.0", "0.1.0")` fails →
`bail!`. Pre-bump runs passed because the example locks matched the older CLI.

**B.** A `///` doc-comment block at `nros/src/lib.rs:375-382` is followed by a
plain `//` comment (Phase-248 C7 relocation note) then a blank line before
`pub mod internals`, which clippy ≥1.96 flags under `-D warnings`. Landed with
the 248/249 churn; orthogonal to A.

## Fix

- **A:** regenerate the stale example Cargo.locks against the `0.5.0` workspace
  (`cargo generate-lockfile` per example, or `nros ws sync`), and/or make the
  build/test harness refresh example locks before `generate-rust`. Optionally
  harden `abi_guard` to still locate the monorepo root lock in the copy-out
  context (so a stale per-example lock can't shadow it). Sweep ALL
  `examples/**/Cargo.lock` for `nros-core 0.1.0`, not just the three above.
- **B:** drop the blank line after the doc comment at `nros/src/lib.rs:382`
  (or convert the trailing `//` note so it doesn't separate the `///` block from
  the item), restoring `check` green.

## Fix landed (2026-06-15, branch `phase-244-native-shape-b`)

- **B (clippy):** moved the Phase-248 C7 `//` note ABOVE the `///` doc block at
  `nros/src/lib.rs` so the doc comment is adjacent to `pub mod internals` (no
  intervening plain comment + blank line). `cargo clippy -p nros
  --no-default-features` clean.
- **A (stale locks):** regenerated all 10 example Cargo.locks that pinned
  `nros-core 0.1.0` → `0.5.0` (6× `examples/qemu-arm-nuttx/rust/*`, 4×
  `examples/stm32f4/rust/*-rtic`) via `nros ws sync` + `cargo generate-lockfile`.
  The two action/service rtic examples also picked up the E3 managed-block
  `nros-rmw-cyclonedds` `[patch.crates-io]` entry their regenerated lock now
  references. Swept the full `examples/**/Cargo.lock` set — these 10 were the only
  `0.1.0` stragglers.

Status flips to `resolved` once `check` + the `platform`/qemu-arm-nuttx cell are
green on `main` post-merge.

## Notes

Neither red is caused by phase-244 example-source cleanup; both are version-bump
/ 248-249 churn fallout on `main`. They block a clean CI-green for any PR. The
phase-244 PR #2 is being ff-merged over these (its own relevant cells —
zephyr-dual-line + the touched trees — are green); close this issue when `check`
and the `platform`/qemu-arm-nuttx cell return green on `main`.
