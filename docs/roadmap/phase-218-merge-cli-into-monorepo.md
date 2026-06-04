# Phase 218 — Merge `nros-cli` into the nano-ros monorepo

**Goal:** Re-merge `github.com/NEWSLabNTU/nros-cli` into the nano-ros
tree as `packages/cli/`, enforcing structural lockstep between the CLI
codegen format and the runtime ABI by construction. Retire the
ten-spot `NROS_VERSION=0.3.7` pin, retire `~/.nros/bin`, retire the
`scripts/install-nros.sh` curl path.

**Status:** Design approved 2026-06-04. **Implementation deferred** —
ongoing work (Phase 214/215/216/217 sweeps, downstream ASI bumps)
actively uses the standalone CLI through the existing
`install-nros.sh` + `~/.nros/bin` shape; flipping that mid-stream would
strand in-flight branches. Schedule after the active phases close.

**Priority:** P2 (developer-experience consolidation; not blocking any
runtime functionality).

**Depends on:**
- Phase 195.D (the original carve-out — this phase reverses its
  distribution decision while keeping the carve-out's architectural
  cleanup intact).
- Phase 214.F.3 (the embedded feature-unification CI guard whose
  no_std discipline the sub-workspace shape protects).

**Design contract:** `docs/superpowers/specs/2026-06-04-cli-monorepo-
merge-design.md`. This phase doc is the implementation slice; the
design doc carries the rationale + UX walkthrough.

---

## Overview

The CLI emits Rust / C / C++ code that targets the `nros-core`,
`nros-c`, and `nros-cpp` runtime ABIs in the same checkout. A version
mismatch between the CLI binary and the runtime crates manifests as a
link error, a struct-layout mismatch, or — worst — silent runtime UB.
The current two-repo distribution (Phase 195.D) makes that mismatch
the default state every time one repo moves before the other; the ten
copies of `NROS_VERSION=0.3.7` are the visible scar.

This phase puts the CLI back inside the tree as a **sub-workspace**
(not a root workspace member), modelled on the existing
`packages/testing/nros-{tests,bench,smoke}/` carve-out. The CLI's
host-only dep surface (clap, askama, syn, ureq) stays isolated from
the runtime workspace's no_std feature-unification view; the Phase
214.F.3 guard does not gain new transitive crates to police.

The CLI binary lives at `packages/cli/target/release/nros`, never in
`~/.nros/bin`. Multiple nano-ros worktrees on one machine each get
their own per-tree CLI with no PATH collisions.

## Architecture

```
nano-ros/                              root workspace (runtime crates)
├── Cargo.toml                         [workspace.members] does NOT list packages/cli/
├── Cargo.lock                         runtime workspace lock
├── activate.sh                        NEW — POSIX-shell env exports (SSoT)
├── activate.fish                      NEW — fish shell env exports
├── .envrc                             one-line `source_env ./activate.sh`
├── packages/
│   ├── cli/                           NEW sub-workspace
│   │   ├── Cargo.toml                 own [workspace]
│   │   ├── Cargo.lock                 own lock
│   │   ├── target/release/nros        per-checkout binary
│   │   ├── nros-cli/                  `nros` binary crate
│   │   ├── nros-cli-core/
│   │   ├── nros-build/
│   │   ├── cargo-nano-ros/
│   │   ├── rosidl-{parser,codegen,bindgen}/
│   │   ├── colcon-cargo-ros2/
│   │   ├── nros-msg-to-idl/
│   │   └── README.md
│   ├── core/                          unchanged
│   ├── testing/                       unchanged
│   └── …
└── scripts/
    ├── install-nros-prebuilt.sh       NEW — tagged-artifact fetch
    └── install-nros.sh                DELETED
```

`scripts/build/cargo.sh::nros_cli_bin` resolution order changes from:
```
$NROS_CLI → PATH → ~/.nros/bin/nros → error
```
to:
```
$NROS_CLI → PATH → packages/cli/target/release/nros → error
```

`just setup` (Tier 0 `minimal`) gains a **first** step that builds the
sub-workspace CLI before any other provisioning runs. Downstream tier
steps assume `nros` is already on PATH via the activate file.

## Work Items

### 218.A — History-preserving lift — landed 2026-06-04

- [x] Ran `git filter-repo --to-subdirectory-filter packages/cli/` on
  a `--no-local` clone of `github.com/NEWSLabNTU/nros-cli` @
  `f092a8c5e` (post-217.B.2). 502 commits rewritten in 0.26s.
- [x] Merged the rewritten tree into nano-ros with `git merge --squash
  --allow-unrelated-histories` (preserves linear history per
  CLAUDE.md), then a single squash commit. nros-cli's prior commit
  history stays preserved on the archived
  `github.com/NEWSLabNTU/nros-cli` repository (218.H follow-up).
- [x] Squash commit `bb123f7e1` names both ancestor SHAs in the body:
  nano-ros pre-merge `6d29e8652` + nros-cli pre-merge `f092a8c5e`.

**Files:** `packages/cli/**` (created), repo history (rewritten under
the new subtree).

### 218.B — Sub-workspace bootstrap — landed 2026-06-04

- [x] `packages/cli/Cargo.toml` carries `[workspace]` listing 9
  sub-crates (the design doc's "ten" was an off-by-one — actual count
  is 9: nros-cli, nros-cli-core, nros-build, cargo-nano-ros,
  rosidl-{parser,codegen,bindgen}, colcon-cargo-ros2, nros-msg-to-idl).
  NOT a root workspace member; root `[workspace].exclude` lists
  `packages/cli` belt-and-suspenders.
- [x] Per-sub-crate `Cargo.toml` adjustments: flattened
  `packages/cli/packages/<crate>/` (filter-repo artifact) up to
  `packages/cli/<crate>/`; patched the one stale `../../third-party/...`
  path-dep in `nros-cli-core` to `../third-party/...`; lifted the
  3 nested `.gitmodules` entries into the root.
- [x] First-time `cargo build --release --manifest-path packages/cli/
  Cargo.toml --bin nros` produces a working `nros 0.4.0` (post-218.J
  baseline) in ~30s on a clean checkout.
- [x] Root workspace's `cargo metadata --no-deps --manifest-path
  Cargo.toml` returns zero `packages/cli/` references; sub-workspace's
  `cargo metadata --no-deps --manifest-path packages/cli/Cargo.toml`
  returns the 10 CLI crates (9 sub-crates + the
  `ros-launch-manifest-types` submodule member).

**Files:** `packages/cli/Cargo.toml`, `packages/cli/*/Cargo.toml` (all
ten sub-crates).

### 218.C — Activation surface — landed 2026-06-04

- [x] `activate.sh` POSIX exports: `NROS_REPO_DIR`, ROS source
  (`/opt/ros/humble/setup.bash`), the nros CLI lookup
  (per-checkout preferred over `~/.nros/bin`), play_launch_parser,
  pinned Phase 176 ninja + make, `.env` overrides, `sdk-env.sh`.
  Single source of truth; bash/zsh detection via `${BASH_SOURCE[0]}`
  / `${(%):-%N}`.
- [x] `activate.fish` mirror — same export set, hand-mirrored (not
  generated). fish-specific .env parser (KEY=value with
  quote-stripping) + bash subshell for `sdk-env.sh` extraction.
- [x] `.envrc` shrunk from ~50 lines to a single `source
  "$PWD/activate.sh"` (plus comment explaining why not the direnv-
  native `source_env`). direnv re-evaluation tracks `activate.sh`
  mtime automatically.
- [x] CLAUDE.md "Environment" paragraph updated by Slot I docs sweep
  (commit `3d5aca6d9`): "Run one of: `direnv allow`,
  `source ./activate.sh`, `source ./activate.fish`".
- [x] `book/src/getting-started/` rewritten by Slot I docs sweep —
  the activate-file flow is the canonical install path; all three
  shells listed.
- [x] Verified `bash -c 'source ./activate.sh; which nros'` resolves
  to the per-checkout binary path (`packages/cli/target/release/nros`)
  ahead of `~/.nros/bin`. The `zpico-sys/build.rs` panic-on-missing-
  `FREERTOS_PORT` canary fires correctly when activation is skipped,
  passes when activated.

**Files:** `activate.sh` (new), `activate.fish` (new), `.envrc`
(rewritten), `CLAUDE.md`, `book/src/getting-started/*.md`,
`docs/reference/environment-variables.md`.

### 218.D — Just recipe wiring

- [x] `justfile` `setup` recipe (Tier 0 path) prepended with a step
  that builds the sub-workspace CLI when the binary is absent or stale
  relative to `packages/cli/Cargo.lock`. _(2026-06-04)_
- [x] New `just setup-cli` private recipe: standalone CLI build,
  callable from the orchestrator and from a user who hit "cargo clean
  evicted my nros". _(2026-06-04)_
- [x] `scripts/build/cargo.sh::nros_cli_bin` resolution order changed
  to the new sequence; clear error message that names `just setup-cli`
  when the binary is missing. _(2026-06-04)_
- [x] `just doctor` reports the CLI binary path + version on the same
  line as the runtime version, so a skew (impossible by construction
  but checked anyway) surfaces in the readiness output. _(2026-06-04)_

**Files:** `justfile`, `just/*.just` (whichever module owns `setup`),
`scripts/build/cargo.sh`.

### 218.E — ABI guard for downstream consumers

- [x] `nros codegen` + `nros generate-rust`: read the target
  workspace's `Cargo.lock`, find resolved `nros-core` version, compare
  to the binary's embedded `env!("CARGO_PKG_VERSION")`. (2026-06-04)
- [x] On mismatch, exit non-zero with the actionable error message in
  the design doc (names both versions + the fix command). (2026-06-04)
- [x] `NROS_SKIP_VERSION_CHECK=1` env opt-out for intentional cross-
  version workflows. (2026-06-04)
- [x] Test: in `packages/cli/nros-cli/tests/`, a fixture with a
  hand-edited `Cargo.lock` pinning an old `nros-core` version asserts
  the guard fires. (2026-06-04)
- [x] Test: with `NROS_SKIP_VERSION_CHECK=1`, the same fixture
  succeeds. (2026-06-04)

**Files:** `packages/cli/nros-cli/src/commands/codegen.rs` (or
wherever the version-resolve happens), `packages/cli/nros-cli/
tests/abi_guard.rs` (new).

### 218.F — Workflow pin removal — landed 2026-06-04

- [x] All ten workflows lose their `NROS_VERSION=0.3.7` line and the
  `curl install.sh` step. (`nros-acceptance.yml` is the deliberate
  carve-out — that lane's whole point is exercising the prebuilt-binary
  path; it carries a TODO pointing at 218.G's
  `scripts/install-nros-prebuilt.sh` re-target.)
- [x] Replaced by a "Build nros CLI" step that runs `cargo build
  --release --manifest-path packages/cli/Cargo.toml --bin nros` AND
  exports `${{ github.workspace }}/packages/cli/target/release` onto
  the job's PATH.
- [x] `actions/cache@v4` keyed on `hashFiles('packages/cli/
  Cargo.lock', 'packages/cli/**/Cargo.toml', 'packages/cli/**/*.rs')`
  caches the CLI's `target/` so PR builds that don't touch the CLI
  tree pay sub-30-second restore. The wider key (lock + manifests +
  sources) makes sure an artifact-only change still invalidates.
- [x] Tagged-release runs fetch the prebuilt artifact (Work Item
  218.G) instead of building from source, exercising the prebuilt
  path on the same lanes downstream users will rely on.
  `nros-acceptance.yml` rewritten 2026-06-04: queries the GitHub
  API for the latest `nros-v*` release, downloads
  `nros-x86_64-unknown-linux-gnu.tar.gz`, sha256-verifies via the
  release's sidecar, installs to `$HOME/.nros/bin/nros` (the
  transitional fallback location post-Phase-218.C activate.sh
  chain). Pre-first-tag the lane gracefully reports
  `[SKIPPED] no nros-v* release reachable yet` via a step-level
  `if: steps.rel.outputs.has_release == 'true'` gate. Trigger
  extended to fire on `nros-v*` tag pushes so a release flip
  immediately exercises the bare-runner contract.

**Files:** `.github/workflows/{host-unit-tests,host-integration-tests,
ci,dep-chain,nros-acceptance,platform-ci,zephyr-dual-line,lint}.yml`.

### 218.G — Release artifact job — landed 2026-06-04

- [x] New job (or extension to the existing release workflow) builds
  the CLI for `{x86_64,aarch64}-{unknown-linux-gnu,apple-darwin}` —
  four target triples. Linux x86_64 + macOS native; Linux aarch64 via
  `cross` (docker-backed cross-compile from ubuntu-22.04). Triggers on
  `nros-v*` tag pushes + `workflow_dispatch`.
- [x] Each artifact is `nros-<triple>.tar.gz` (binary + LICENSE-APACHE
  + LICENSE-MIT + README) plus a `.sha256` sidecar.
- [x] Attached to the GitHub release as assets (via
  `softprops/action-gh-release@v2`).
- [x] `scripts/install-nros-prebuilt.sh` fetches the artifact matching
  `git describe --tags --abbrev=0`, verifies the sha256, installs into
  `packages/cli/target/release/nros`. Fall-through if no tag is
  reachable: prints a clear message + suggests the source build
  (`cargo build --release --manifest-path packages/cli/Cargo.toml
  --bin nros` — equivalent to the `just setup-cli` shorthand).
- [x] `scripts/install-nros.sh` (the legacy nros-cli installer) gains
  a `DEPRECATED — Phase 218.A` block + delegates to
  `install-nros-prebuilt.sh` when on a `nros-v*` tag, else errors with
  a 218.A pointer (legacy path preserved behind
  `NROS_LEGACY_INSTALL=1` for downstream pins yet to migrate; full
  removal in Phase 218.H).

**Files:** `.github/workflows/release-nros-cli.yml` (new),
`scripts/install-nros-prebuilt.sh` (new), `scripts/install-nros.sh`
(DEPRECATED block + delegation gate).

### 218.H — Existing repo decommission — landed 2026-06-04

- [x] In `github.com/NEWSLabNTU/nros-cli`: README rewritten as a
  redirect notice pointing at
  `github.com/NEWSLabNTU/nano-ros/tree/main/packages/cli` + the Phase
  218 docs. Local commit `6ae5b01` on `chore/218-h-redirect-notice`
  → maintainer pushed + merged to nros-cli main on 2026-06-04.
- [x] Open issues / PRs: **N/A** — no open issues on `nros-cli` at
  archive time. No migration work needed.
- [x] GitHub repo settings → "Archive this repository". Done by
  maintainer on 2026-06-04. The repo is now read-only; issue / PR
  history preserved.
- [x] In nano-ros: doc / script / workflow references to the old repo
  URL swept by Slot I (docs) + Slot F+G (workflows + bootstrap) +
  Slot D (justfile + cargo.sh). Remaining `github.com/NEWSLabNTU/
  nros-cli` URL references are intentional: historical phase docs
  (`docs/roadmap/archived/phase-195-*`, `phase-217-arm-fvp-*`,
  `phase-218-*` itself), the Phase 218 design spec, the new
  `packages/cli/README.md` (its sibling — preserved verbatim from
  the lift), and the `nros-cli/README.md` redirect (final commit
  prior to archive).

**Files:** `nros-cli/README.md` (in the OTHER repo — final commit),
nano-ros docs sweep (Work Item 218.I covers the in-tree side).

### 218.I — Docs sweep

- [x] `CLAUDE.md` "Build" section: "`nros setup` is the canonical
  provisioner" paragraph updates to reference `packages/cli/` instead
  of the external repo. The `scripts/install-nros.sh` mention
  retires; replaced with the activate-file flow. (2026-06-04)
- [x] `docs/development/sdk-tiers.md`: Tier 0 `minimal` step list
  gains a "build nros CLI from `packages/cli/`" bullet. (2026-06-04)
- [x] `docs/reference/environment-variables.md`: `NROS_CLI` becomes
  the documented override; `~/.nros/bin` removed from the fallback
  chain. (2026-06-04)
- [x] `book/src/getting-started/`: rewrite the "Install the CLI"
  section around the activate-file flow. (2026-06-04)
- [x] `book/src/internals/`: add a "CLI lives in the monorepo" note
  pointing at this phase doc and the design spec. (2026-06-04)
- [x] `.vscode/settings.json`: `rust-analyzer.linkedProjects` lists
  both the root `Cargo.toml` and `packages/cli/Cargo.toml` (same
  pattern as the existing testing sub-workspaces, if not already
  present). (2026-06-04)

**Files:** `CLAUDE.md`, `docs/development/sdk-tiers.md`,
`docs/reference/environment-variables.md`,
`book/src/getting-started/*.md`, `book/src/internals/*.md`,
`.vscode/settings.json`.

## Acceptance

A fresh `git clone https://github.com/NEWSLabNTU/nano-ros.git`
followed by **either** `direnv allow` **or** `source ./activate.sh`
(POSIX) **or** `source ./activate.fish` (fish), then `just setup`,
produces a working `nros` binary at `packages/cli/target/release/nros`
on the user's PATH with **zero curl calls** and **zero references to
`~/.nros/bin`**. `nros --version` reports the nano-ros tag the
checkout is on. Running `just ci` from that state passes.

A `grep -rE 'NROS_VERSION=|~/.nros/bin|install-nros.sh' .github/
scripts/ docs/ CLAUDE.md` returns zero matches (modulo the one
expected match in `install-nros-prebuilt.sh`'s own help text).

The release workflow attaches `nros-{x86_64,aarch64}-{linux,macos}.
tar.gz` assets to a tag build; `install-nros-prebuilt.sh` fetches and
verifies them.

`github.com/NEWSLabNTU/nros-cli` shows as archived with a redirect
README; clones to its URL still succeed (preserving any
checkouts-by-SHA in third-party tooling) but the repo is read-only.

Phase doc retired to `archived/` when every checkbox above is
flipped.

## Notes

**Scheduling.** This phase is structurally non-disruptive once it
lands (every user-facing surface is documented; the activate file
makes the new flow drop-in for both direnv and non-direnv users), but
the *transition* requires all active branches to rebase across the
sub-workspace introduction. Wait until the active 21x sweeps + the ASI
↔ nano-ros bump (memory `project_asi_nano_ros_bump`) land before
flipping. The earliest sensible window is when no `feature/phase-21*-
*` branches are open against `origin`.

**Why sub-workspace, not root member.** The runtime workspace's
`no_std` feature-unification view (Phase 214.F.3) is the project's
most fragile invariant — a single workspace member adding a
`std`-activating dep without target-gating turns the embedded build
red on a path the test matrix doesn't always exercise. The CLI's
deps (clap, askama, syn, ureq, …) are aggressively std-only; making
them workspace siblings would either (a) require target-gating every
single one or (b) effectively turn off the F.3 guard. Sub-workspace
keeps the two surfaces categorically separate.

**Why per-checkout, not `~/.nros/bin`.** Confirmed during
brainstorming UX walkthrough: contributors with multiple nano-ros
worktrees (phase branches, ASI integration trees, downstream forks)
need each tree to point at its own CLI. A global install would
silently version-skew across trees the moment the user `cd`s. The
per-checkout shape makes the activate file the single switch — no
"`which nros`" surprise across cwd boundaries.

**Why archive instead of delete.** GitHub `delete` purges issue/PR
history; archive preserves it read-only. The nros-cli repo has
non-trivial history (the original carve-out from Phase 195.D, the
`nros ws sync` design discussions, etc.) worth keeping searchable.

**Open at design time, deferred to implementation:**
- Per-sub-crate `Cargo.toml` rewrites if the lifted manifests carry
  any nros-cli-root-relative `[patch]` tables that need re-rooting.
- Whether `cargo-nano-ros` retains its current crates.io publish
  story or moves to a `cargo install --git` flow. Decided at the
  release window for the first post-merge tag.
- If the merged tree's `just setup` cold time grows past the
  ≤5 min `default` SDK-tier policy, the CLI build step gets a tier
  carve-out so it can run on-demand instead of unconditionally.

### 218.J — JetPack-style bundle versioning — landed 2026-06-04

Added after 218.A–I landed: the implementation lift produced a CLI
binary at `nros-cli 0.3.7` (inherited from the standalone repo's
tags) acting on runtime crates at `nros-core 0.1.0` — the
Phase 218.E ABI guard fired on the merged tree itself. Versioning
discussion settled on a single bundle version (the JetPack model):
no crate publishes to crates.io; the project ships as a tagged release
with CLI binaries + a runtime checkout; one version label rides the
whole thing.

- [x] **218.J.1** — Adopt `0.4.0` as monorepo-merge baseline (signals
      discontinuity from `nros-v0.3.7` standalone tag without
      claiming post-1.0 stability). Bumped
      `[workspace.package].version` in both `Cargo.toml` and
      `packages/cli/Cargo.toml`; mass sed-bumped every
      `version = "0.1.0"` path-dep pin in member crates → `"0.4.0"`
      so `cargo update --workspace` resolves cleanly across the
      runtime + CLI sub-workspace.
- [x] **218.J.2** — `scripts/check-version-lockstep.sh` extracts
      `[workspace.package].version` from both manifests; errors with
      both versions printed when they diverge. Wired into
      `.github/workflows/lint.yml` after `check-decoupling`.
- [x] **218.J.3** — `just release-bump <X.Y.Z>` recipe sed-bumps both
      files atomically + runs the lockstep guard. Validates SemVer
      shape (`^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?$`).
- [x] **218.J.4** — `docs/development/versioning.md` documents the
      bundle model: no crates.io publish, git-tag-driven releases,
      single label moves runtime + CLI together. Cross-refs the
      ABI guard (218.E), release workflow (218.G), and lockstep
      check.
- [x] **218.J.5** — `scripts/bootstrap.sh::install_nros_prebuilt` (the
      `bootstrap nros` subcommand) updated to the Phase 218 acquisition
      chain: PATH check → `install-nros-prebuilt.sh` on tagged
      checkouts → `cargo build --release --manifest-path
      packages/cli/Cargo.toml --bin nros` on branch / development
      checkouts. Removes the dependency on the now-deprecated
      `install-nros.sh` curl path that 218.F flagged as a regression
      on non-tagged checkouts.

**Deferred to follow-up:**
- Sweep every workspace member's `Cargo.toml` to switch hardcoded
  `version = "0.4.0"` to `version.workspace = true`. ~50 crates;
  cargo workspace inheritance breaks for standalone (own-`[workspace]`)
  crates so they need to keep the hardcoded form. Not blocking.
- Defensive `publish = false` on every member that doesn't already
  carry it. ~80 crates touched. Belt-and-suspenders only.

**Files:** `Cargo.toml`, `packages/cli/Cargo.toml`,
`scripts/check-version-lockstep.sh` (new),
`docs/development/versioning.md` (new), `justfile`,
`.github/workflows/lint.yml`, `scripts/bootstrap.sh`, ~50
`packages/*/*/Cargo.toml` (mass version bump).
