# Phase 253 — CI lane tiering + local-runnable jobs

Status: **In progress (substance done; cosmetic + CI-validation remaining)** ·
Design: [docs/development/ci-workflow-reorg.md](../development/ci-workflow-reorg.md).
Related: #57 (the cadence-cancellation root), #69/#70/#73 (gate-content reds the
SSoT exposed), phase-251 (`--allow-multiple-definition` gate).

> **Goal.** Every CI job is runnable locally by a named `just` recipe, CI yml is a
> thin `just <recipe>` caller (so local ⇄ CI can't drift), and each push carries a
> bounded load whose run actually COMPLETES. Heavy lanes were cancelled mid-run
> every ~2-3 min under the multi-agent main-push cadence (#57 root) — so a direct
> push to `main` now triggers ONLY the fast buildless gate; everything heavier runs
> on PR + nightly with `cancel-in-progress` only on PR (a started nightly finishes).

## Lane inventory (current)

| workflow | tier / trigger | local recipe | cancel-in-progress |
| --- | --- | --- | --- |
| **check** | push → `check-fast` (buildless ~1 min); PR + nightly + dispatch → full `check` + `check-no-std` | `just check-fast` / `just check` | true |
| host-unit-tests | PR + nightly + dispatch | `just test-unit` | PR only |
| host-integration-tests | PR + nightly + dispatch | `just test-integration` (+ fixture builds) | false |
| platform-ci | PR (changed-platform) + nightly + dispatch | `just <plat> setup/build/test` | PR only |
| zephyr-dual-line | PR (zephyr paths) + nightly + dispatch | `just zephyr build-one/ci-both/check-copy-out` | false |
| sdk-index-gate | PR (sdk paths) + dispatch | `just check-sdk-index` | true |
| scaffold-journey | push + PR (cli paths) | `just scaffold-journey` | true |
| colcon-parity | push + PR (template path) | `just colcon-parity` | true |
| nros-acceptance | release tags `nros-v*` + nightly + dispatch | `just acceptance` (local from-source) | false |
| deploy-book | push (book/api paths) | `just doc` / `just book` (build part) | false (publish: don't interrupt git-push) |
| release-nros-cli | tags `nros-v*` + dispatch | `cargo build --release --bin nros` | — |
| build-ci-base-image / build-zephyr-ci-image | push (ci/docker paths) | — (CI-only image push) | false |

`just check` = `check-fast` (buildless: fmt/clang-format/ABI/manifest/convention/
cargo-tree/example-fmt) + `check-build` (compile: workspace+example clippy, feature
combos, riscv32 no_std, nros-tests source gates, staticlib link-proof, dep-chain).

## Done

- `check.yml` thinned + split (fast/build tiers, push=fast); `ci.yml` deleted
  (folded as `just check-no-std`).
- Heavy/medium lanes moved off per-push → PR + nightly + dispatch
  (host-integration, host-unit, platform-ci, zephyr-dual-line, nros-acceptance).
- `just check` is the fast-gate SSoT (wraps every `check.yml` gate); local recipe
  mirror of every standalone CI job; `[group("ci")]` + the recipe-org convention
  block in the justfile header.
- Gate-content reds fixed: #69 (dep-chain own-feature detect + package.xml gate),
  #70 (staticlib test → single-archive), #73 (build-profile edge classification).

## Remaining (non-blocking)

- [ ] **Thin scaffold-journey + colcon-parity** to call `just scaffold-journey` /
      `just colcon-parity` (they still inline the scripts). Path-scoped → no cadence
      issue; pure SSoT consistency.
- [ ] **Optional file rename/merge** into tier names (`pr-checks`/`host-tests`/
      `nightly`/…). The triggers already implement the tiers, so functionally moot.
- [ ] **CI validation** — confirm on a real run that per-push `check-fast`
      completes + the nightly lanes (host-integration / platform-ci / zephyr) run
      green. Needs a quiet window or a manual `workflow_dispatch`; locally validated
      (`just check-fast` green ~67 s).

## Acceptance

- A direct push to `main` triggers only fast lanes (`check-fast`, + the
  path-scoped publish/scaffold/colcon) and they complete under the push cadence.
- Heavy lanes run + COMPLETE on PR + nightly (no cadence cancellation).
- Every CI job has a `just` recipe; CI yml calls it (no enumerated-gate drift).
