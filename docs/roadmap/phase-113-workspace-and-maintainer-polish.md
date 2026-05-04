# Phase 113: Workspace + Maintainer Polish

**Goal:** Curate the maintainer-facing surface so new users see a clean `just --list` and never trip over the sibling-`nano-ros-workspace` directory layout.

**Status:** Not Started
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

- [ ] **113.A.1** Audit every recipe in `justfile` and `just/*.just`. Tag each user-vs-internal in a tracking table.
- [ ] **113.A.2** Add `[private]` attribute to internal recipes; rename to `_`-prefixed where missing.
- [ ] **113.A.3** Add `just --list-all` (or `just maintainer-list`) wrapper showing everything.
- [ ] **113.A.4** Sweep `book/src/reference/build-commands.md` and per-platform getting-started pages — recipes there must be user-verb only.
- [ ] **113.B.1** Change default `west init` path in `scripts/zephyr/setup.sh` to `zephyr-workspace/` inside the repo.
- [ ] **113.B.2** Update `.gitignore` (`/zephyr-workspace/` already there per CLAUDE.md — verify and adjust).
- [ ] **113.B.3** Rewrite `book/src/getting-started/zephyr.md` setup section.
- [ ] **113.B.4** Verify env override still works (`NROS_ZEPHYR_WORKSPACE` or similar).
- [ ] **113.B.5** Cleanup script for users on old layout: `tools/migrate-zephyr-workspace.sh` symlinks the old sibling into the in-tree path.

**Files:**
- `justfile`, `just/*.just` (annotate)
- `scripts/zephyr/setup.sh`
- `.gitignore`
- `book/src/getting-started/zephyr.md`
- `book/src/reference/build-commands.md`
- `tools/migrate-zephyr-workspace.sh` (new)

---

## Acceptance criteria

- `just --list` shows ≤ 30 recipes, all named after user verbs.
- `just --list-all` shows everything, including internal.
- Fresh `git clone && just setup && just zephyr setup` produces a working `zephyr-workspace/` *inside* the repo.
- `book/src/getting-started/zephyr.md` no longer mentions a sibling directory.

## Notes

- Risk: hiding a recipe a user is already typing breaks their muscle memory. Mitigation: keep names; only flip the `[private]` attribute. `just <hidden-name>` still works.
- This phase can run concurrently with Phase 111; no dependency.
