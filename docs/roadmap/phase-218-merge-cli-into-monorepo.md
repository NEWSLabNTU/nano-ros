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

### 218.A — History-preserving lift

- [ ] Run `git filter-repo --to-subdirectory-filter packages/cli/` on
  a fresh clone of `github.com/NEWSLabNTU/nros-cli`.
- [ ] Merge the rewritten history into nano-ros as a single commit
  (`-s ours -X subtree=packages/cli` or `git merge --allow-unrelated-
  histories` + linear rebase, depending on what keeps the bisect tree
  clean — pick at implementation time).
- [ ] Commit message references both ancestor SHAs (the last nros-cli
  commit pre-archive + the nano-ros pre-merge HEAD).

**Files:** `packages/cli/**` (created), repo history (rewritten under
the new subtree).

### 218.B — Sub-workspace bootstrap

- [ ] Author `packages/cli/Cargo.toml` with `[workspace]` listing the
  ten sub-crates (members + workspace-level deps lifted from the
  nros-cli root manifest). NOT a root workspace member.
- [ ] Per-sub-crate `Cargo.toml` adjustments: any path-deps that
  referenced the old nros-cli layout root re-resolve via the new
  `packages/cli/` root.
- [ ] First-time `cargo build --release --manifest-path packages/cli/
  Cargo.toml --bin nros` from a clean checkout produces a working
  binary.
- [ ] Root workspace's `cargo build` / `cargo check` does NOT pull in
  the CLI sub-workspace (verify by inspecting `cargo metadata` —
  the CLI's crate names must not appear in root metadata).

**Files:** `packages/cli/Cargo.toml`, `packages/cli/*/Cargo.toml` (all
ten sub-crates).

### 218.C — Activation surface

- [ ] `activate.sh` POSIX exports: `PATH`, `NROS_CLI`, plus the SDK
  paths the existing `.env` currently carries. Single source of truth.
- [ ] `activate.fish` mirror exports for fish users. Hand-written, not
  generated — twenty lines max.
- [ ] `.envrc` becomes one effective line: `source_env ./activate.sh`
  (plus any direnv-specific comments preserved from the existing
  file).
- [ ] CLAUDE.md "Environment" paragraph updates: direnv is one of three
  supported activators, not the only one.
- [ ] `book/src/getting-started/` gains a "Activate the workspace"
  section listing all three paths (direnv, bash/zsh, fish).
- [ ] Verify `zpico-sys/build.rs` no longer panics with `FREERTOS_PORT
  not set` when the user `source`d `activate.sh` instead of using
  direnv (the panic is the canary for missing activation).

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
- [ ] Tagged-release runs fetch the prebuilt artifact (Work Item
  218.G) instead of building from source, exercising the prebuilt
  path on the same lanes downstream users will rely on. (Deferred —
  the prebuilt path is wired via `scripts/install-nros-prebuilt.sh`
  but no workflow currently invokes it; will be folded into
  `nros-acceptance.yml` once 218.G ships its first tagged release.)

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

### 218.H — Existing repo decommission

- [ ] In `github.com/NEWSLabNTU/nros-cli`: final commit replaces
  README with a redirect notice pointing at
  `github.com/NEWSLabNTU/nano-ros/tree/main/packages/cli`.
- [ ] Open issues / PRs: migrated to nano-ros with a `cli` label, or
  closed with a redirect comment.
- [ ] GitHub repo settings → "Archive this repository" (preserves
  issue/PR history read-only).
- [ ] In nano-ros: every doc, script, and workflow reference to the
  old repo URL bumps to the new `packages/cli/` path.

**Files:** `nros-cli/README.md` (in the OTHER repo — final commit),
nano-ros docs sweep (Work Item 218.I covers the in-tree side).

### 218.I — Docs sweep

- [ ] `CLAUDE.md` "Build" section: "`nros setup` is the canonical
  provisioner" paragraph updates to reference `packages/cli/` instead
  of the external repo. The `scripts/install-nros.sh` mention
  retires; replaced with the activate-file flow.
- [ ] `docs/development/sdk-tiers.md`: Tier 0 `minimal` step list
  gains a "build nros CLI from `packages/cli/`" bullet.
- [ ] `docs/reference/environment-variables.md`: `NROS_CLI` becomes
  the documented override; `~/.nros/bin` removed from the fallback
  chain.
- [ ] `book/src/getting-started/`: rewrite the "Install the CLI"
  section around the activate-file flow.
- [ ] `book/src/internals/`: add a "CLI lives in the monorepo" note
  pointing at this phase doc and the design spec.
- [ ] `.vscode/settings.json`: `rust-analyzer.linkedProjects` lists
  both the root `Cargo.toml` and `packages/cli/Cargo.toml` (same
  pattern as the existing testing sub-workspaces, if not already
  present).

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
