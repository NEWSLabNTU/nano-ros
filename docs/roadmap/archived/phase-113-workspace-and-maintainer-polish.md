# Phase 113: Workspace + Maintainer Polish

**Goal:** Curate the maintainer-facing surface so new users see a clean `just --list` and never trip over the sibling-`nano-ros-workspace` directory layout.

**Status:** Complete
**Priority:** Low (polish; high leverage per hour)
**Depends on:** none
**Related:** `docs/research/sdk-ux/SYNTHESIS.md` UX-5, UX-9

---

## Overview

Two small, low-risk fixes the UX research flagged repeatedly:

1. **`just --list` shows 60+ recipes**, many internal (`_cmake-cargo-stale-guard`, `install-local-posix`, `refresh-cmake-cargo`, `_orchestrate`, …). New users see noise. ESP-IDF's `idf.py --help` shows ~25 user verbs and hides the rest behind `--all`.
2. **`nano-ros-workspace` is a sibling directory** of the repo, accessed via `zephyr-workspace -> ../nano-ros-workspace` symlink. New users clone into `~/Code/nano-ros/` and end up with `~/Code/nano-ros-workspace/` as a peer — confusing.

---

## Architecture

### A. Curated `just --list`

Audit `justfile` + `just/*.just`. For each recipe, decide:

- **User verb** (visible in `just --list`): kept public, named after a verb the user types.
- **Internal recipe** (hidden in `just --list`, visible in `just --list-all`): annotated `[private]`, or prefixed `_` (just convention). Examples: stale-guards, internal install steps, orchestration glue.

Add a top-level `just --list-all` recipe that prints everything. Default `just --list` curated.

### B. In-tree Zephyr workspace

`west init -l . zephyr-workspace/` — the workspace lives in a (gitignored) subdirectory of the repo, not a sibling. The `west.yml` already at repo root drives the same modules. The sibling-directory env override stays for users who already have it.

`scripts/zephyr/setup.sh` updated. `book/src/getting-started/zephyr.md` rewritten so the path is `nano-ros/zephyr-workspace/`, not `nano-ros-workspace/`. `.gitignore` updated.

Migration: existing setups keep working (the env override remains). New `just zephyr setup` runs prefer the in-tree path.

---

## Work Items

- [x] **113.A.1** Audited every top-level justfile recipe.
- [x] **113.A.2** Added `[private]` to 19 internal recipes (workspace/embedded/feature checks, format-{c,cpp,python,workspace}, check-{c,cpp,python,workspace,...}, install-local-posix, refresh-cmake-cargo, init-test-logs, build-zenoh, check-zenoh, build-zenohd, clean-zenohd, build-zenoh-pico, generate-{rcl-interfaces,lifecycle-msgs}, doc-{c-check,rmw-cffi,platform-cffi}). `just --list` drops from 89 → 65 lines.
- [x] **113.A.3** Added `just list-all` recipe printing all 84 recipes including private ones (awk-extracted from justfile).
- [x] **113.A.4** Swept `book/src/` + `README.md` for stale top-level recipes that became `[private]` or never existed. Rewrites cover: per-platform `build-examples-*` → `just <plat> build`, `test-{qemu,zephyr,zephyr-xrce,...}` → `just <plat> test*`, `docker-*` → `just docker <recipe>`, `qemu-help` → `just qemu help`, `setup/teardown/status-qemu-network` → `just qemu *-network`, `test-qemu-wcet` → `just qemu test-wcet`, `build-zenoh-pico-{arm,riscv}` → `just qemu build-zenoh-pico` / `just esp32 build*`. 10 doc files updated. Reverted `build-zenohd` / `clean-zenohd` / `build-zenoh-pico` from `[private]` (referenced from README). Test-tier table in `internals/contributing.md` rewritten to match actual `test-unit` + `test-integration` composition.
- [x] **113.B.1** `scripts/zephyr/setup.sh` defaults to `$repo/zephyr-workspace/`. Honors `NROS_ZEPHYR_WORKSPACE` env override. Auto-detects pre-existing legacy sibling install at `$parent/$name-workspace/`.
- [x] **113.B.2** `/zephyr-workspace` already in `.gitignore`.
- [x] **113.B.3** Rewrote `book/src/getting-started/zephyr.md` overview, step 3, troubleshooting, and update sections.
- [x] **113.B.4** `NROS_ZEPHYR_WORKSPACE` env override threaded through `setup.sh`, `just/zephyr.just` (`ZEPHYR_WORKSPACE` var), `cortex-a9-rust-patch.sh`, `native-sim-ipproto-ip-patch.sh`. Legacy `ZEPHYR_WORKSPACE` retained as fallback.
- [x] **113.B.5** Migration helper at `scripts/zephyr/migrate-workspace.sh` (with `--dry-run`).

**Files touched:**
- `justfile` — 19 recipes marked `[private]`, new `list-all` recipe.
- `just/zephyr.just` — `ZEPHYR_WORKSPACE` honors `NROS_ZEPHYR_WORKSPACE` and prefers in-tree path; `build-fixtures` no longer hard-codes `../nano-ros-workspace`.
- `scripts/zephyr/setup.sh` — default workspace path flipped, env override added, legacy auto-detect.
- `scripts/zephyr/migrate-workspace.sh` — new (legacy-sibling → in-tree mover).
- `scripts/zephyr/cortex-a9-rust-patch.sh`, `scripts/zephyr/native-sim-ipproto-ip-patch.sh` — env-override + in-tree-default resolution chain.
- `book/src/getting-started/zephyr.md`, `book/src/reference/build-commands.md`, `book/src/user-guide/troubleshooting.md` — sibling-workspace refs removed.

---

## Acceptance criteria

- [x] `just --list` shows ≤ 50 user-facing recipes (currently 47 + 17 platform mods).
- [x] `just list-all` shows every recipe (84 total, includes private).
- [x] Fresh `git clone && just setup && just zephyr setup` produces a working `zephyr-workspace/` *inside* the repo.
- [x] `book/src/getting-started/zephyr.md` overview describes the in-tree layout; sibling directory only appears in legacy/migration notes.

## Notes

- Risk: hiding a recipe a user is already typing breaks their muscle memory. Mitigation: keep names; only flip the `[private]` attribute. `just <hidden-name>` still works.
- This phase can run concurrently with Phase 111; no dependency.
