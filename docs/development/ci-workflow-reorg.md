# CI workflow reorg — plan (review draft)

Status: **DRAFT for review** (Step B). Step A (local `just` mirrors of every CI
job + `ci-fast`) landed in `52de496b2`. This doc proposes the workflow-side
reorg; nothing here is applied yet.

Goals (from the request):
1. Every CI task runnable locally via a convenient named `just` recipe.
2. CI yml = thin `just <recipe>` wrappers over a single source of truth, so the
   local command and the CI job can't drift.
3. Bounded per-push load; heavy lanes must actually *complete* (today they're
   cancelled mid-run under the multi-agent main-push cadence).

## Current state — 14 workflows

| Workflow | Trigger | Load | Local recipe |
| --- | --- | --- | --- |
| check | push+PR (broad) | fast | `just check` (⚠ drifted — see below) |
| ci (no_std) | push+PR (core) | med | `just check-no-std` (new) |
| host-unit-tests | push+PR | med | `just test-unit` |
| host-integration-tests | push+PR | **heavy 45–90m** | `just test-integration` (+ fixture builds) |
| platform-ci | push+PR+nightly | **very heavy** 6-plat | `just <plat> setup/build/test` |
| zephyr-dual-line | push+PR | **very heavy** 10×2 | `just zephyr build-one/ci-both/check-copy-out` |
| colcon-parity | push+PR (template) | fast | `just colcon-parity` (new) |
| scaffold-journey | push+PR (cli) | med | `just scaffold-journey` (new) |
| sdk-index-gate | PR (sdk) | fast (network) | `just check-sdk-index` (new) |
| nros-acceptance | push main + tags | med | `just acceptance` (new, local variant) |
| deploy-book | push main (broad) | heavy | `just doc` / `just book` (build part) |
| build-ci-base-image | push (ci/docker) | image | — (CI-only; optional `just docker`) |
| build-zephyr-ci-image | push (ci/docker) | image | — (CI-only) |
| release-nros-cli | tags `nros-v*` | heavy 4 targets | `cargo build --release --bin nros` |

## Problems

**P1 — per-push load explosion + cancellation.** A single `packages/**` push fires
check + ci + host-unit + host-integration + platform-ci (×6) + zephyr-dual-line +
deploy-book ≈ 10+ jobs. Heavy ones take 45–90 min; the next push (~10 min later)
cancels them via `cancel-in-progress: true`. **Heavy lanes never finish on
direct-to-main** (root of issue #57's chronic red). Triggers also overlap badly —
nearly everything keys on `packages/**`.

**P2 — `just check` ≠ `check.yml` (drift).** They run different gate sets both
ways, so CI can be green while `just check` is red (and vice versa):
- in `just check` only: `check-no-direct-kernel-alloc`, `check-board-manifest-drift`,
  `check-weak-symbols`, `native::check`, `check-python`.
- in `check.yml` only: `check-decoupling`, `check-version-lockstep`, and 3
  test-gates (`platform_header_matrix`, `cross_libc_precedence_gate`,
  `zephyr_prjconf_requirements`).

**P3 — local-runnability gaps.** Closed in Step A (5 new recipes).

## Target — ~7 workflows, tiered by trigger

| New workflow | Absorbs | Trigger | concurrency | Local |
| --- | --- | --- | --- | --- |
| **pr-checks.yml** | check + ci(no_std) + sdk-index + scaffold + colcon | push+PR (broad) | cancel ✓ | `just ci-fast` (+ `just check-sdk-index` etc. when prereqs present) |
| **host-tests.yml** | host-unit + host-integration | PR + nightly | cancel ✓ on PR | `just test` / `just test-integration` |
| **nightly.yml** | host-integration(full) + platform-ci(6) + zephyr-dual-line | `schedule` + `workflow_dispatch` + path-scoped PR | **cancel ✗** | `just test-all`, `just <plat> ci`, `just zephyr ci-both` |
| **release.yml** | release-nros-cli → nros-acceptance | tags `nros-v*` | — | `cargo build --release --bin nros`, `just acceptance` |
| **images.yml** | build-ci-base-image + build-zephyr-ci-image | push (ci/docker/**) | cancel ✗ | (optional `just docker build-*-image`) |
| **docs.yml** | deploy-book | push main (book/api paths) | cancel ✗ | `just doc` / `just book` |

Core principle: **fast lanes per-push** (complete in minutes, cancellation is
fine); **heavy lanes on schedule/dispatch/PR with `cancel-in-progress: false`** so
each run completes (no cadence cancellation). Per-push load = the fast gate only.

## The reconciliation that makes wrappers possible (fixes P2)

Make ONE source of truth for "what the fast gate runs". Option chosen:
- `just check` = the full local lint set (authoritative).
- `pr-checks.yml` runs **`just ci-fast`** + the per-job recipes (`just check-sdk-index`,
  `just scaffold-journey`, `just colcon-parity`) — i.e. CI calls the SAME recipes a
  developer runs. The bespoke per-gate `run:` steps + the 3 inline `cargo test`
  gates in `check.yml` move INTO `just check` (add `check-decoupling`,
  `check-version-lockstep`, and the 3 header/precondition test-gates as recipes),
  so `just check` becomes a superset and CI = `just check` exactly. No drift.

Open item folded in: `check-no-direct-kernel-alloc` is **hard-by-default and
correct** (no real bypasses) but red on a false positive — its `\bk_malloc\b`
regex matches the word inside `why:` doc-strings in
`tests/zephyr_prjconf_requirements.rs`. Fix = add `packages/testing/` to the
gate's `EXCLUDE_RE` (tests don't allocate through the funnel). Lands with the
reconciliation so `just check` / `ci-fast` go green.

## Migration steps (each its own commit, reviewable)

1. Alloc-gate false-positive fix (`EXCLUDE_RE += packages/testing/`); `just check`
   green. (Unblocks `ci-fast`.)
2. Reconcile `just check` ⊇ `check.yml` gate set (add the missing recipes); make
   `check.yml` call `just check` (thin).
3. Fold check + ci + sdk-index + scaffold + colcon → `pr-checks.yml` (thin
   `just` wrappers).
4. host-unit + host-integration → `host-tests.yml` (PR + nightly split).
5. New `nightly.yml`: host-integration(full) + platform-ci + zephyr-dual-line,
   `schedule` + `dispatch` + path-scoped PR, `cancel-in-progress: false`. Remove
   their per-push triggers.
6. release-nros-cli + nros-acceptance → `release.yml`; both image builds →
   `images.yml`; deploy-book → `docs.yml` (narrow paths).

## Out of scope / kept CI-only
- Docker image push (`docker buildx … --push`), `gh release` upload, GitHub Pages
  push — inherently CI credentials/glue; the *buildable* part is local
  (`mdbook build`, `cargo build --release`, `docker build`).
