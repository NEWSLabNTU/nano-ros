# Phase 218 — merge `nros-cli` into the nano-ros monorepo

**Status:** design approved 2026-06-04, awaiting implementation plan
**Owner:** aeon
**Supersedes:** Phase 195.D (CLI carve-out into standalone repo)

## Goal

Bring `github.com/NEWSLabNTU/nros-cli` back into the nano-ros tree as
`packages/cli/`. **Lockstep the CLI's codegen format with the runtime
ABI by construction** — one checkout = one CLI version = one runtime
ABI. No version-pin matrix to police, no `NROS_VERSION=0.3.7`
hardcoded in ten workflow files.

The codegen format the CLI emits (Rust/C/C++ message structs, FFI
shapes, generated trampolines) is structurally bound to the `nros-core`
+ `nros-c` + `nros-cpp` crates in the same checkout. Today they live
in two repos with a hand-bumped version pin — a known footgun. The
monorepo merge makes the binding the only possible state.

## Non-goals

- Versioning each CLI sub-crate independently on crates.io. The merge
  reduces to a single workspace version; we are not publishing the
  CLI's individual library crates separately.
- Reintroducing the `~/.nros/bin` install path. The CLI binary lives
  inside the checkout; a contributor with multiple nano-ros worktrees
  gets per-tree CLIs with no PATH collisions.
- Cross-tag CLI binaries (one CLI that works against many nano-ros
  versions). Lockstep means single-tag use.

## Out of scope (separate work)

- Downstream consumer (Autoware-port, ASI) Cargo manifests updating
  their `nros` pin. They land in their own repos after this lands.
- Publishing `cargo-nano-ros` to crates.io under its new path; decided
  at release time, not in the merge commit.

## Approved design

### Tree layout

```
packages/cli/                    NEW sub-workspace
  Cargo.toml                     [workspace] members = [nros-cli, nros-build, ...]
  Cargo.lock                     own lock (NOT root workspace)
  nros-cli/                      `nros` binary crate
  nros-cli-core/
  nros-build/                    codegen lib used by consumer build scripts
  cargo-nano-ros/
  rosidl-{parser,codegen,bindgen}/
  colcon-cargo-ros2/
  nros-msg-to-idl/
  README.md                      consumer-facing CLI docs
```

The sub-workspace shape mirrors the existing `packages/testing/nros-
{tests,bench,smoke}/` carve-out: each has its own `[workspace]` table,
keeping the host-only dep surface (clap, askama, syn, ureq) isolated
from the runtime workspace's no_std feature-unification view. The
Phase 214.F.3 embedded-feature-unification CI guard does not need to
police any new transitive crate.

Migration mechanism: `git filter-repo` lift of `nros-cli` into
`packages/cli/` preserving history; merged into nano-ros `main` as a
single commit referencing both ancestor SHAs in the commit message.

### Build + install

The CLI binary lives at `packages/cli/target/release/nros` after a
build. The binary is **per-checkout**, never installed globally.

`just setup` (Tier 0 `minimal`) runs:
```bash
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
```
as its first step, before any other SDK provisioning. Downstream
recipes resolve the binary via `scripts/build/cargo.sh::nros_cli_bin`:
```
$NROS_CLI → PATH → packages/cli/target/release/nros → error
```
Note the absence of `~/.nros/bin` — that fallback is retired with this
phase.

`scripts/install-nros.sh` is retired. It is replaced by
`scripts/install-nros-prebuilt.sh` which fetches the tagged release
artifact (see [Release pipeline](#release-pipeline)) for downstream
users who do not want to build from source.

### Environment activation

CLAUDE.md's `direnv allow` requirement is **relaxed**: direnv becomes
one of several supported activators. The single source of truth is a
shell-agnostic `activate.sh` that exports the env (`PATH`,
`NROS_CLI`, `NROS_HOME` if any). All three entry points hit the same
file:

```
.envrc          → source_env ./activate.sh             (direnv users; direnv stdlib)
activate.sh     → exports for bash/zsh/sh              (POSIX shells)
activate.fish   → exports for fish                      (fish users)
```

Bootstrap UX per shell:
- direnv: `direnv allow` once after clone.
- bash/zsh: `source ./activate.sh` (add to `.bashrc`/`.zshrc` for
  per-cd auto-activation if wanted).
- fish: `source ./activate.fish`.
- Anything else: read the file, set the two exports by hand. The
  variables are short (`PATH` + `NROS_CLI`); no opaque magic.

Per-checkout PATH switching falls out naturally — each tree's
`activate.sh` exports `PATH=$PWD/packages/cli/target/release:$PATH`, so
switching cwd flips PATH if the user has shell auto-activation hooked,
or stays on whichever tree they last activated otherwise.

### Versioning

Single version everywhere. `packages/cli/Cargo.toml`'s
`[workspace.package].version` is hand-synced with the root
workspace's version on every release. The CLI sub-crates inherit via
`version.workspace = true` inside `packages/cli/`.

`nros --version` reports the nano-ros tag the binary was built from:
```
$ nros --version
nros 0.4.0 (nano-ros@4546e9ade, codegen abi 2026-06)
```
The `codegen abi <YYYY-MM>` token is a stable label that increments
only when the codegen output shape changes in a way that breaks
existing generated code (the same discipline rmw_zenoh applies to its
wire keyexpr — see CLAUDE.md `rmw_zenoh_interop`). It is informational
for users diagnosing a skew; the ABI guard (next section) is what
actually enforces compatibility.

### ABI guard (downstream skew protection)

The structural-lockstep premise covers contributors and CI. A
downstream consumer who pulled `nros = { git = nano-ros, tag = v0.4.0 }`
into their own workspace can still end up with a *different* `nros`
binary on PATH (an older install, a sibling checkout, a copy from a
colleague's tarball). Catch this loud at codegen time:

`nros codegen` and `nros generate-rust`:
1. Read the target workspace's `Cargo.lock`.
2. Find the resolved version of `nros-core` (the canonical anchor
   crate).
3. Compare to its own embedded `NROS_CORE_VERSION` constant baked at
   build time (`env!("CARGO_PKG_VERSION")` of the matched workspace).
4. On mismatch, fail with:
   ```
   error: nros binary version 0.5.0 does not match the target
          workspace's pinned nros-core version (0.4.0)

   fix: install the CLI matching this workspace:
     cargo install --git https://github.com/NEWSLabNTU/nano-ros \
                   --tag v0.4.0 nros-cli
   or use the per-checkout binary at
     ./packages/cli/target/release/nros
   ```

The check is opt-out via `NROS_SKIP_VERSION_CHECK=1` for the rare
intentional cross-version case (e.g. local CLI iteration against an
older runtime branch).

### Release pipeline

A new job in the existing release workflow builds the CLI for four
target triples per tag:
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

Each artifact is `nros-<triple>.tar.gz` with the binary + `LICENSE` +
`README.md`, accompanied by `nros-<triple>.tar.gz.sha256`. They attach
to the release as assets.

`scripts/install-nros-prebuilt.sh` reads the current checkout's tag
(`git describe --tags --abbrev=0`), fetches the matching artifact,
verifies the sha256, and installs into
`packages/cli/target/release/nros` so the per-checkout shape stays
unchanged. Fall-through: if no tag is reachable (mid-tag commit, dev
branch), the script prints the closest ancestor tag and offers
`cargo build --release` as the alternative.

CI workflows (10 lanes) update their `nros` provisioning step:
- Tag builds + main builds: build from source (validates the build
  path itself). `actions/cache` keys on `packages/cli/Cargo.lock`'s
  sha256, scoping caches per-CLI-revision.
- PR builds on feature branches: fetch the most recent tag's
  prebuilt; falls back to source build if the CLI tree changed in
  this PR (cache key changes).

Net effect: existing CI's per-job 30-90s `curl install.sh` becomes
either a ~5s tarball fetch (cache hit) or a ~3-minute source build
(cache miss, only when the CLI tree changed).

### Existing repo decommission

`github.com/NEWSLabNTU/nros-cli`:
1. Final commit: README replaced with redirect notice pointing at
   `github.com/NEWSLabNTU/nano-ros/tree/main/packages/cli`.
2. Archive (read-only) via GitHub repo settings.
3. Open issues/PRs are migrated to nano-ros with a `cli` label or
   closed with a redirect comment.

In-tree references to `nros-cli` (workflows, scripts, docs) bump to
`packages/cli/` paths in the same merge commit:
- `scripts/install-nros.sh` deleted, replaced by
  `scripts/install-nros-prebuilt.sh`.
- Ten workflows lose their `NROS_VERSION=0.3.7` lines; replaced by
  `just setup` (or the prebuilt-fetch step described above).
- CLAUDE.md's `nros setup` paragraph updates: "The nros CLI lives at
  `packages/cli/`; `just setup` builds it as its first step".
- `docs/development/sdk-tiers.md`: Tier 0 `minimal` gains a "build
  nros CLI from `packages/cli/`" bullet.

### Roadmap doc

New: `docs/roadmap/phase-218-merge-cli-into-monorepo.md` with the
following work-item slice:

- **A. History lift.** `git filter-repo` lift + history-preserving
  merge commit. Files touched: none outside the new tree.
- **B. Sub-workspace bootstrap.** `packages/cli/Cargo.toml`
  `[workspace]` + `Cargo.lock` + per-sub-crate manifest tweaks so
  `cargo build` from `packages/cli/` builds standalone.
- **C. Activation surface.** `activate.sh` + `activate.fish` + `.envrc`
  one-liner. CLAUDE.md update: direnv-optional.
- **D. Just recipe wiring.** `just setup` first step;
  `just setup-cli` shortcut; `scripts/build/cargo.sh::nros_cli_bin`
  resolution order change.
- **E. ABI guard.** `nros codegen` / `nros generate-rust` version
  check against target workspace's `Cargo.lock`.
- **F. Workflow pin removal.** Ten workflows' `NROS_VERSION=0.3.7`
  lines deleted; replaced by the build-or-fetch logic with
  `actions/cache` keyed on `packages/cli/Cargo.lock`.
- **G. Release artifact job.** Four-triple matrix build + tarball +
  sha256 attached to release.
- **H. Existing repo archive.** README redirect commit + GitHub repo
  archive flip + issue migration script.
- **I. Docs sweep.** CLAUDE.md, `docs/development/sdk-tiers.md`,
  `book/src/getting-started/`, `docs/reference/environment-
  variables.md` all reference the new path.

**Acceptance criteria:**

A fresh `git clone https://github.com/NEWSLabNTU/nano-ros.git` followed
by **either** `direnv allow` **or** `source ./activate.sh` (POSIX) or
`source ./activate.fish` (fish), then `just setup`, produces a working
`nros` binary on the user's PATH with **zero curl calls** and **zero
references to `~/.nros/bin`**. `nros --version` reports the
nano-ros tag the checkout is on. Running `just ci` from that state
passes.

## UX scenarios validated against the design

(Done during brainstorming on 2026-06-04; reproduced here for spec
self-review reference.)

| Scenario | Issue | Resolution in design |
|---|---|---|
| A.1 First `direnv allow` before `just setup` | binary not yet built | `.envrc`/`activate.sh` tolerate missing PATH entries; `just setup-cli` shortcut + clear error from cargo wrapper |
| A.2 User without direnv | direnv pre-req gated bootstrap | Relaxed: `activate.{sh,fish}` are first-class siblings to `.envrc` |
| A.3 Cold-clone time +5-15 min | CLI build cost | Prebuilt tarball fast path via `install-nros-prebuilt.sh` for users who don't need to iterate on the CLI |
| A.4 `cargo clean` inside packages/cli | binary evicted | `just setup-cli` shortcut + clear cargo-wrapper error pointing at it |
| B.1 Downstream Cargo consumer | needs `nros` on PATH outside the nano-ros tree | `cargo install --git nano-ros --tag <T> nros-cli` documented; prebuilt tarball alternative |
| B.2 Downstream pinned-vs-installed skew | possible after install | ABI guard reads target workspace `Cargo.lock`, errors with actionable fix line |
| C.3 Sibling worktree never set up | mysterious "command not found" | Cargo wrapper error names the missing `just setup-cli` step |
| D.1 CI rebuilds CLI per PR | 10 jobs × 5+ min cold | `actions/cache` keyed on `packages/cli/Cargo.lock`; tag builds fetch prebuilt |
| E.1 rust-analyzer two-workspace | linkedProjects needs both | `.vscode/settings.json` lists both `Cargo.toml`s; same pattern as existing testing sub-workspaces |
| F.1 `nros ws sync` knowing its tree | was a known pain | Strictly improved — `current_exe()` ancestors locate the root unambiguously |

## Open follow-ups (post-merge)

- Downstream consumers (Autoware-port, ASI) bump their CLI install
  instructions to `cargo install --git nano-ros nros-cli`. Tracked in
  their own repos; not part of this phase.
- The `cargo nano-ros` cargo-subcommand subcrate may need re-publishing
  to crates.io under its new path. Decided at release time.
- If the merged tree's CI cold time grows beyond the SDK-tier policy
  (≤5 min wall clock per `default` tier), revisit splitting the CLI
  build out of `just setup` into a separate `just setup-cli` recipe
  that the orchestrator skips when the binary is already cached.
