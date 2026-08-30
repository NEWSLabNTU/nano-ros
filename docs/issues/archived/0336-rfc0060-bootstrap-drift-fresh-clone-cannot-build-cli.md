---
id: 336
title: "Post-RFC-0060 bootstrap drift: a fresh clone cannot build the CLI — bootstrap.sh, activate, AGENTS.md, doctor and 9 book pages still init the retired ros-launch-manifest submodule"
status: resolved
type: bug
severity: high
area: build, docs
related: [issue-0285, rfc-0060]
---

## Finding (audit 2026-07-28, P1)

RFC-0060 replaced the two vendored launch submodules with a single
`packages/cli/third-party/ros-launch-resolve` (which nests
`ros-launch-manifest` + `parser` one level deeper). The root `CLAUDE.md` was
updated; **every satellite that bootstraps or documents the old shape was not.**

### The break

`scripts/bootstrap.sh:189` — `ensure_cli_submodules()`:

```bash
local sub="packages/cli/third-party/ros-launch-manifest"
if [[ -f "${REPO_ROOT}/${sub}/types/Cargo.toml" ]]; then return 0; fi
```

- `.gitmodules` declares only `packages/cli/third-party/ros-launch-resolve`
  (verified) — the retired path is not a submodule at all any more.
- The CLI's path deps run *through* the resolver
  (`packages/cli/nros-cli-core/Cargo.toml:46-49`:
  `../third-party/ros-launch-resolve/third-party/ros-launch-manifest/{types,model,sched}`).
- So the guard file never appears, `git submodule update --init <retired path>`
  does nothing useful, and `cargo build` fails on the path deps.

It appears to work on the maintainer's host **only** because two retired
submodule worktrees are still sitting on disk
(`packages/cli/third-party/{play_launch_parser,ros-launch-manifest}`, untracked
post-RFC-0060 residue). Deleting them turns this from latent into visible.

### The documentation copies (9 sites, identical dead command)

`activate.sh:75`, `activate.fish:50`,
`book/src/getting-started/{installation.md:148, first-node-rust.md:23,
first-node-cpp.md:23, first-node-c.md:22, workspace-bringup.md:27,
workspace-node-pkgs.md:28, workspace-entry-pkg.md:27,
workspace-from-app-node.md:89}`, `book/src/reference/cli.md:24`.

`AGENTS.md:285` is worse than stale: it says to init the CLI submodules
"(NOT `--recursive`)", which post-RFC-0060 leaves the CLI unbuildable because
the pinned submodule nests two levels. The same bullet still advertises
PATH-wiring `play_launch_parser`, and :286 claims `just doctor` FAILs on it.
(The "never `--recursive` from a worktree" landmine at :294 is about the
*unscoped* form and needs reconciling, not deleting.)

### `just doctor` checks the wrong things

`just/workspace.just:294` gates on `play_launch_parser` on PATH ("`nros plan`
shells out to it" — no such call site exists any more) and **never checks
`nros-launch-resolve`**, which `nros ws sync` hard-requires (`cmd/ws.rs:471`
bails without it) and `just setup` builds unconditionally (`justfile:2436`).
Net effect: doctor reports a green tree that cannot sync a workspace, and
reports MISSING for a binary the CLI no longer invokes.

### The book still teaches the 0285 footgun

`book/src/getting-started/workspace-entry-pkg.md:128` documents "`nros sync` …
using `play_launch` from PATH" with a copy-pasteable `play_launch resolve …`
command; also `workspace-cpp.md:242,:266`, `workspace-bringup.md:295`,
`user-guide/component-and-entry-pkg.md:120`. A reader with ROS 2's unrelated
`play_launch` on PATH reproduces issue 0285 verbatim.

### Stale crate map

`packages/cli/CLAUDE.md:43` still lists nested submodules as
`third-party/{play_launch_parser, ros-launch-manifest}` and has no row for
`nros-launch-resolve`; `packages/cli/nros-cli-core/Cargo.toml:36` still asserts
resolution shells out to the `play_launch_parser` binary.

## Fix

1. `bootstrap.sh`: `sub="packages/cli/third-party/ros-launch-resolve"`, guard on
   `third-party/ros-launch-resolve/resolve/Cargo.toml`, keep the **scoped**
   `--init --recursive` (the nested rlm + parser are now required).
2. Make the bootstrap command an SSoT — one book include + one shell helper —
   instead of nine copies.
3. `just doctor`: add `[OK]/[MISSING] nros-launch-resolve → just
   setup-launch-resolve`; demote the `play_launch_parser` check to the test tier
   with its real justification (the `launch_synth` / `self_bringup` /
   `orchestration_includes` PATH probes).
4. Book: `nros ws sync` is the only user-facing verb; describe the helper as
   `nros-launch-resolve` resolved by absolute path, never a bare name.
5. Fix `AGENTS.md:285-286` and `packages/cli/CLAUDE.md:33-43` + that Cargo.toml
   comment, citing RFC-0060.
6. **Gate it:** `git grep -n 'third-party/ros-launch-manifest' -- ':!packages/cli/third-party'`
   must be empty. That single grep would have caught all nine doc copies and the
   bootstrap script.

## Resolution (2026-07-28)

All six surfaces fixed, plus the gate the issue asked for.

1. **`scripts/bootstrap.sh`** — `ensure_cli_submodules()` now targets
   `packages/cli/third-party/ros-launch-resolve`, guards on
   `…/ros-launch-resolve/resolve/Cargo.toml`, and keeps the scoped
   `--init --recursive` (required: the pin nests rlm + parser). Verified
   mechanically: the target IS declared in `.gitmodules`, the guard file
   resolves, and the init is recursive — the three properties whose absence made
   the old function a silent no-op.
2. **12 documented copies** of the dead command rewritten in one pass
   (`README.md`, `activate.sh`, `activate.fish`, 8 book pages,
   `book/src/reference/cli.md`, `docs/development/ci-conventions.md`,
   `packages/cli/README.md`). The audit findings doc keeps its copy on purpose —
   it is the record of what was broken.
3. **`AGENTS.md`** — the "(NOT `--recursive`)" instruction is corrected and now
   states explicitly why the scoped form must be recursive while the *unscoped*
   landmine still stands, so the two lines stop contradicting each other.
4. **`just doctor`** — new `[OK]/[MISSING] nros-launch-resolve` check (with a
   distinct hint when the submodule itself is uninitialised), and
   `play_launch_parser` demoted in the comments to the TEST-tier prereq it
   actually is. Verified live: doctor reported `[MISSING] nros-launch-resolve` on
   a tree that could not sync, then `[OK]` after `just setup-launch-resolve`.
5. **Book** — `nros sync` is now the only user-facing verb; the copy-pasteable
   `play_launch resolve …` blocks are gone and the text names
   `nros-launch-resolve` as an absolute-path helper (issue 0285).
6. **`packages/cli/CLAUDE.md` + `nros-cli-core/Cargo.toml`** — crate map gains an
   `nros-launch-resolve` row, the nested-submodule line reflects the single pin,
   and the Cargo comment stops claiming the CLI shells out to
   `play_launch_parser` (nothing in `packages/cli/` spawns it).

### The gate

`scripts/check-retired-submodule-refs.sh`, wired into `just check fast`. It fails
on any live reference to a retired path (currently
`packages/cli/third-party/{ros-launch-manifest,play_launch_parser}`), excluding
the docs that legitimately *record* the drift and references that pass THROUGH
the live pin. One grep would have caught all 21 sites at review time — this issue
and the `.github` half of #337 are the same missed sweep.

Extend `RETIRED[]` whenever a path is retired: a path is retired once, so a
permanent entry costs microseconds and makes the next sweep un-partial.

Verification: `just check fast` green (gate included), `just doctor` green after
setup, `bash -n` clean on both scripts.

**Not fixed here** (belongs to #337, already resolved upstream): the eight
`.github/` workflow references — verified clean on this tree, the workflows now
init `ros-launch-resolve`.
