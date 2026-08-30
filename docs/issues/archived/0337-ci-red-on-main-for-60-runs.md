---
id: 337
title: "`pr-checks` red on main for 60+ consecutive runs: a source-free gate calls `cargo fmt`, which needs generated artifacts a fresh checkout cannot have"
status: resolved
type: bug
area: build
related: [issue-0336, issue-0272, issue-0288, rfc-0048, phase-312, phase-315]
---

## Finding (2026-07-28)

Remote `pr-checks` on `main` has failed **every one of the last 60 runs**, back
to 2026-07-27T17:17 (the furthest the API would page). Nobody noticed, because
everyone — including this session, all day — validates with `just check`
locally and pushes on a green local result.

Two independent breakages, both in the `check` job.

## Breakage 1 — a source-free gate that needs generated artifacts

```
error: could not load Cargo configuration
  failed to load config include `../../../../../nros-patch.toml`
    from `/__w/nano-ros/nano-ros/examples/native/rust/action-client-async/.cargo/config.toml`
  No such file or directory (os error 2)
error: recipe `check-example-fmt` failed with exit code 101
```

`check-example-fmt` ran `cargo fmt --check` per example leaf. `cargo fmt`
shells out to `cargo metadata`, which loads the leaf's `.cargo/config.toml`,
which carries `include = ["…/nros-patch.toml"]` — the central `[patch.crates-io]`
file that **`nros sync` generates and `.gitignore` excludes** (RFC-0048 W9;
absolute paths, so it cannot be committed). CI never runs `nros sync`, so the
file never exists, so every leaf fails before a single line is formatted.

### The placement is the real defect

`check-example-fmt` lives in **`check-fast`**, whose contract is stated in the
justfile:

> *"BUILDLESS, SOURCE-FREE gates only … No cargo build/clippy/test **AND no
> `cargo tree`/metadata** (which would need the workspace … to resolve)."*

`cargo fmt` violates that contract. The gate was placed in the one tier that
promises never to resolve the workspace, and then resolved the workspace.

### It got a second failure mode this week

Phase-315 added generated `nros-selection` facade crates as **`[dependencies]`
path-deps of workspace members**. A member's path-dep must exist for `cargo
metadata` to resolve *at all*, so after pulling phase-315 the same call fails
locally too, until `nros sync` is run across **12** workspaces. That is what
made this visible: the same root cause, arriving from a second direction.

## Breakage 2 — workflows pointed at a submodule that no longer exists

```
error: pathspec 'packages/cli/third-party/ros-launch-manifest'
       did not match any file(s) known to git
```

**8 references across 4 workflows** (`pr-checks`, `host-tests`, `nightly`,
plus a comment in `docs`). This is fallout from phase-312 (RFC-0060), which
replaced `packages/cli/third-party/{play_launch, ros-launch-manifest,
play_launch_parser}` with the single `third-party/ros-launch-resolve` pin. The
tree was swept; `.github/workflows/` was not.

**Same drift class as issue 0336**, filed independently by another session,
which found the retired path in `scripts/bootstrap.sh`, nine doc copies,
`AGENTS.md` and the book — but not in the workflows. The two are complementary:
0336 owns the bootstrap/doc surface, this issue owns CI. Neither is a subset of
the other, and 0336 remains open.

## Fix

**Breakage 1 — call `rustfmt` directly.** Formatting needs no dependency graph.
Invoking `rustfmt --check --edition <ed>` per tracked `.rs` file removes
`cargo metadata` from the path entirely: no config include, no generated
crates, no `nros sync` prerequisite. The gate now honours its tier and cannot
regress this way again.

Two details worth keeping:

- The edition is read from each manifest rather than assumed. The tree is not
  uniform — 7 leaves under `examples/zephyr/` are not edition 2024 (they are
  excluded by this recipe's filter today, but assuming would be a trap).
- File discovery stays index-driven (`git ls-files`), preserving the existing
  "tracked files only" discipline so `generated/` and build trees cannot leak
  in.

**Breakage 2 — repoint the 8 references** at
`packages/cli/third-party/ros-launch-resolve`. `--recursive` still reaches
`ros-launch-manifest`, which that repo vendors, so the crates the jobs need are
unchanged.

## Receipts

- `just check example-fmt` → rc=0.
- **With `nros-patch.toml` deleted** (a fresh-checkout simulation, i.e. exactly
  CI's state) → still rc=0. Before this change that was the failing case.
- **Mutation-checked:** with a deliberate formatting violation appended to
  `examples/native/rust/talker/src/main.rs`, the gate returns rc=1 and reports
  the diff. Restored clean.
- `git submodule update --init --recursive --depth 1
  packages/cli/third-party/ros-launch-resolve` → rc=0; the path is registered
  in `.gitmodules`. All four workflow YAMLs parse.

## Follow-up: `check-cli-fmt` had the identical defect

Fixing `check-example-fmt` moved the failure one recipe along rather than
clearing the job. `check-cli-fmt` was `cd packages/cli && cargo fmt --check`,
and on CI's push lane:

```
`cargo metadata` exited with an error: failed to load manifest for workspace
member `packages/cli/nros-cli-core`
  failed to load manifest for dependency `ros-launch-manifest-model`
  failed to read `…/ros-launch-resolve/third-party/ros-launch-manifest/model/Cargo.toml`
```

Same shape: a `check-fast` gate calling `cargo fmt`, needing a dependency graph
that a source-free lane cannot resolve — here the NESTED rlm submodule inside
`ros-launch-resolve`. I had looked at this recipe while fixing the first one
and judged it safe because it is a single resolvable workspace locally. That
was wrong: locally the submodules are initialised, on the push lane they are
not.

Converted the same way, with one care: `cargo fmt` covers workspace MEMBERS
only, so the replacement reads the member list from the manifest and skips
`tests/fixtures/` — those are separate packages, several deliberately on
editions 2018/2021, and were never in scope. Per-member edition resolution
handles `edition.workspace = true` by falling back to the workspace's 2024.

Receipts: 11 members / 143 files, rc=0; mutation-checked (a violation in
`nros-cli/src/main.rs` gives rc=1 and a reported diff, restored clean).

## Why it survived 60 runs

Nothing points at remote CI. CLAUDE.md says *"Green CI locally BEFORE pushing —
don't iterate on remote CI"*, which is good advice that quietly assumes local
and remote agree. They had not agreed for over a day, and a green `just check`
reads as permission to push.

The generalisable rule, and the same one issues 0314 and 0319 landed on: **a
gate that only ever runs in a lane nobody watches will be red and nobody will
know.** Here the lane was remote CI itself. Worth considering a cheap
`gh run list --branch main --limit 1` check as part of the pre-push habit,
since it costs one command and would have caught this a day earlier.
