---
id: 65
title: CI `check` cell red on main — stale `nros/platform-posix` feature combo (248-C5c fallout) + nros-cpp clang-format drift
status: resolved
type: bug
area: build
related: [phase-248, issue-0066]
resolved_in: 2026-06-15
---

> **RESOLVED (2026-06-15).** A: dropped `platform-posix` from the two `nros`
> feature combos at `justfile:1240-1243` (`nros` umbrella lost it in 248-C5c;
> nros-c/nros-cpp keep it) — `just check-workspace-features` green. B: reformatted
> 5 `nros-cpp/include/nros/*.hpp` (action_client/action_server/polling_action_client/
> polling_action_server/tick_ctx) with the CI-pinned **clang-format 17.0.5**
> (`check.yml:117`); the local v22 had masked the drift — `clang-format --dry-run
> --Werror` clean across all headers. Both validated locally.

## Symptom

The `check (fmt + clippy + structural gates)` CI cell is RED on `main`,
independent of any PR. Two distinct sub-recipes fail (the cell runs
`check-workspace-all` → `check-workspace-features` → `check-cpp`):

### A — `check-workspace-features`: nros built with a removed feature

```
cargo clippy --quiet -p nros --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble"
error: the package 'nros' does not contain this feature: platform-posix
  help: packages with the missing feature: nros-platform, nros-rmw-zenoh, ...
error: recipe `check-workspace-features` failed (exit 101)
```

### B — `check-cpp`: clang-format violations

```
packages/core/nros-cpp/include/nros/action_client.hpp:108:80: error: code should be clang-formatted [-Wclang-format-violations]
  ... :125:64, :166:57, :194:53  (reinterpret_cast<uint8_t (*)[16]>(goal_id) lines)
```

(`check-workspace-all` — clippy + nightly fmt — is green after issue-0066's
`nros/lib.rs` empty-line fix.)

## Root cause

**A.** Phase-248 C5c retired the `platform-*` features from the `nros` umbrella
crate (agnosticism convergence). But the feature-combo gate still enumerates the
old surface:

```
justfile:1241  cargo clippy ... -p nros ... --features "std,rmw-cffi,platform-posix,ros-humble"
justfile:1242  (same with ros-iron)
```

`nros` no longer has `platform-posix`, so clippy aborts before linting. The
recipe's combo list was not updated when the feature was dropped.

**B.** `nros-cpp/include/nros/action_client.hpp` drifted out of clang-format
compliance (the `reinterpret_cast<uint8_t (*)[16]>(goal_id)` argument wrapping at
lines 108/125/166/194). Landed without a `just format` pass.

Both predate and are disjoint from phase-244 (the example-source-cleanliness diff
touches neither `justfile` nor `nros-cpp`); `check` was already failing on `main`
before phase-244 merged (e.g. run on `feat(249 P3)`).

## Fix

- **A:** drop `platform-posix` from the `nros` feature strings at `justfile:1241`
  + `:1242` (and sweep the recipe for any other `nros/platform-*` combo). The
  platform feature now lives on `nros-platform` / the board / RMW crates, not the
  umbrella — if a posix-platform clippy combo is still wanted, target
  `nros-platform --features platform-posix`, not `nros`.
- **B:** run `just format` (or `clang-format -i`) on
  `nros-cpp/include/nros/action_client.hpp`; re-gate with `just check-cpp`.

## Notes

Sibling of [issue 0066](0066-ci-red-stale-example-locks-abi-guard-and-clippy-empty-line.md)
(other CI reds from the 0.5.0 bump / 248-249 churn). Close when the `check` cell
is green on `main`.
