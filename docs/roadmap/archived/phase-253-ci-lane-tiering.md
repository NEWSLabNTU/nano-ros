# Phase 253 — CI lane tiering + local-runnable jobs

Status: **Done** (13→6 tier files, SSoT recipes, bounded per-push, CI-validated) ·
Design: [docs/development/ci-workflow-reorg.md](../development/ci-workflow-reorg.md).
Related: #57 (the cadence-cancellation root), #69/#70/#73 (gate-content reds the
SSoT exposed), phase-251 (`--allow-multiple-definition` gate).

> **Goal.** Every CI job is runnable locally by a named `just` recipe, CI yml is a
> thin `just <recipe>` caller (so local ⇄ CI can't drift), and each push carries a
> bounded load whose run actually COMPLETES. Heavy lanes were cancelled mid-run
> every ~2-3 min under the multi-agent main-push cadence (#57 root) — so a direct
> push to `main` now triggers ONLY the fast buildless gate; everything heavier runs
> on PR + nightly with `cancel-in-progress` only on PR (a started nightly finishes).

## Lane inventory (6 tier files)

13 workflows → **6 tier-named files**, one job per former lane. Triggers + job-level
concurrency implement the tiers; a `changes` (dorny/paths-filter) job path-gates the
narrow jobs so each runs only when its paths move (on a PR); push/schedule/dispatch
run all of a file's jobs.

| file | jobs (← former workflow) | trigger | concurrency |
| --- | --- | --- | --- |
| **pr-checks.yml** | `check` (← check; push=`check-fast`, PR/nightly=`check`+no-std), `scaffold-journey`, `colcon-parity`, `sdk-index` | push + PR + nightly(0 2) + dispatch | workflow, cancel ✓ |
| **host-tests.yml** | `unit` (← host-unit), `integration` (← host-integration) | PR + nightly(0 3) + dispatch | per-job (unit cancel-on-PR; integration ✗) |
| **nightly.yml** | `platform` (← platform-ci, cron 0 7), `zephyr-example-matrix`/`zephyr-dual-line-summary`/`zephyr-copy-out` (← zephyr-dual-line, cron 0 5) | PR + 2 crons + dispatch | per-job (platform cancel-on-PR; zephyr ✗); `github.event.schedule` routes each cron to its job set |
| ~~release.yml~~ | `build`/`release` (← release-nros-cli, tag/dispatch), `fresh-machine` (← nros-acceptance, tag+nightly 0 6+dispatch) — **deleted 2026-07-11** (phase-288 D1/D2: no prebuilt channel; never shipped a release, and its acceptance drove the Phase-222-removed `nros build`) | tags `nros-v*` + cron(0 6) + dispatch | per-job (acceptance ✗) |
| **images.yml** | `ci-base`/`zephyr` (← build-ci-base-image / build-zephyr-ci-image) | push (ci/docker paths) + dispatch | per-job, ✗ |
| **docs.yml** | `deploy` (← deploy-book) | push (book/api paths) + dispatch | `deploy-book`, ✗ |

Local recipes unchanged: `just check-fast`/`check`, `test-unit`, `test-integration`,
`<plat> setup/build/test`, `zephyr build-one/ci-both/check-copy-out`, `check-sdk-index`,
`scaffold-journey`, `colcon-parity`, `acceptance`, `doc`/`book`.

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

- [x] **CI validation** — done via `workflow_dispatch` on all 6 tier files (2026-06-17).
      Merge STRUCTURE confirmed correct: `changes` path-gate routes the narrow jobs
      right; build-tier skip-on-push works; per-cron `github.event.schedule` guard +
      job-level concurrency function; every job runs in the right order. Lane results:
      **docs ✓** (full rustdoc), **host-tests unit ✓**, **nightly platform matrix
      freertos/threadx_linux/esp32 ✓ + zephyr 3.7 cells ✓**. Validation also CAUGHT
      the provisioned-source-cache bug (reverted — see "Done since"). Residual reds
      are pre-existing parallel-wave CONTENT debt, not the merge: pr-checks
      `check-build`/`check-no-std` (a #73 safety-talker compile error + a no_std
      `can't find crate for std` regression — the old `check` lane was already red on
      these) and the `zephyr 4.4` setup cells. CI-caught reds now filed as issues:
      **0077** (check-no-std: serde_core pulls std on thumbv7em) + **0078** (nightly
      zephyr 4.4 setup ENOSPC). The trivial ones were fixed inline (safety_e2e.rs
      rustfmt; safety-listener.c clang-format; 3× redundant rustdoc link targets;
      docs lane ros-launch-manifest submodule init).

### Done since
- **Push lane de-coupled from the CLI build.** `check-fast` needs no nros CLI (its
  only CLI-touching gates — board-manifest-drift, profile-board-mirror — skip
  gracefully when absent). The push tier now provisions px4-rs via `git submodule
  update --init` (it's a submodule; cargo-tree needs it) and SKIPS the ~min CLI
  build + heavy-source provisioning (build tier / non-push only). Push lane ≈
  checkout + clang-format + submodule + `check-fast` ≈ 1-2 min — completes under
  the rapid push cadence (was ~4-5 min, cancelled).

- **Prereq prep cached, not image-baked.** The user flagged `nros setup --source`
  (git-fetch + checkout of each `-sys` submodule) as the slow part of the build
  lanes and asked about baking prereqs into the CI image. Image-baking was
  REJECTED: a CLI or prereq-config change forces a manual image rebuild + carries
  a staleness hazard (a build runs against whatever the image last baked). Chosen
  instead — two auto-invalidating `actions/cache`s, keyed so a change forces a miss
  with no manual step:
  - **CLI binary cache** (already in place): `nros-cli-${{ runner.os }}-${{
    hashFiles('packages/cli/Cargo.lock','packages/cli/**/Cargo.toml',
    'packages/cli/**/*.rs') }}`, shared identically across check / host-unit /
    host-integration / platform-ci. Has `restore-keys` (cargo tolerates a warm
    non-exact target → sub-30s relink).
  - **Provisioned-source cache** — ATTEMPTED, then REVERTED (validation found it
    breaks provisioning). Design was: key on the recorded submodule SHAs
    (`git ls-tree HEAD <paths>` → `.ci-source-pins`), exact-key-only so a pin bump
    misses + refetches. The pin-key/staleness logic was sound, but the structure
    is not: `nros setup --source` provisions via git submodules, and `actions/cache`
    archives only the source WORKING TREE — not its `.git/modules/<path>` git-dir.
    On a cache HIT the restored dir has a dangling `.git` gitlink, so nros's
    `git -C <dir> fetch <sha>` fails with `fatal: not a git repository:
    …/.git/modules/…` (seen on the first host-tests dispatch). Exact-key prevents
    *stale* content but not this *structural* breakage — any hit poisons the dir.
    Reverted from all 4 lanes; only the CLI-binary cache (a plain target dir, no
    submodule) remains. **To reintroduce safely:** add `.git/modules/<path>` to each
    cache `path:` alongside the source dir so the gitlink resolves on restore (then
    nros idempotent-skips) — untested; revisit deliberately, not under push churn.

- **scaffold-journey + colcon-parity thinned to `just` callers.**
  scaffold-journey.yml: `just scaffold-journey` (its `setup-cli` prereq reuses the
  cached `packages/cli/target`; the lane keeps only the ros-launch-manifest
  submodule init + ROS source). colcon-parity.yml: installs `just` (no base image
  on that lane) then `just colcon-parity`. Both lanes now run the SAME recipe a
  developer runs — no inline-script drift. Path-scoped, so no cadence concern.

- **13 workflows merged into 6 tier files.** check + scaffold-journey + colcon-parity
  + sdk-index-gate → **pr-checks.yml** (a `changes` paths-filter gates the 3 narrow
  jobs; `check` always runs); host-unit + host-integration → **host-tests.yml**
  (job-level concurrency keeps unit cancel-on-PR + integration always-completes);
  platform-ci + zephyr-dual-line → **nightly.yml** (two crons, a
  `github.event.schedule` guard routes 0 7→platform / 0 5→zephyr); release-nros-cli
  + nros-acceptance → **release.yml** (`fresh-machine` `needs: release` so a tag
  build's assets exist before acceptance fetches them; `always()`+result-guard lets
  the nightly cron run acceptance standalone); the two image builds → **images.yml**
  (`changes`-gated, job-level env per image); deploy-book → **docs.yml** (`git mv`,
  history-preserved). No branch-protection required-checks reference the old job
  names (verified — none configured), so the rename breaks nothing. README CI badge
  (was pointing at the deleted `ci.yml`) + all active filename refs (justfile,
  Dockerfiles, AGENTS.md, versioning.md, zephyr-version-support.md) repointed;
  archived phase docs left as historical record. `actionlint` clean.

## Acceptance

- A direct push to `main` triggers only fast lanes (`check-fast`, + the
  path-scoped publish/scaffold/colcon) and they complete under the push cadence.
- Heavy lanes run + COMPLETE on PR + nightly (no cadence cancellation).
- Every CI job has a `just` recipe; CI yml calls it (no enumerated-gate drift).
