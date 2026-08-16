---
id: 627
title: "The CLI freshness closure was wrong in both directions: blind to `workspace = true` deps, and watching 17 crates the CLI never compiles"
status: resolved
type: bug
severity: high
area: build, cli
related: [issue-0604, issue-0561, issue-0466, issue-0196, phase-330]
---

## Symptom

Two, and they look unrelated:

* **An edit to `packages/core/nros-rmw` or `packages/core/nros-core` left
  `nros source-stamp` reporting FRESH.** `just setup-cli` then skipped the
  rebuild while reporting success, and the tree kept running a CLI built from
  older sources. No message says so — that is the whole difficulty.
* **An edit to `packages/core/nros-node` — or `nros-log`, or any platform port
  — re-staled the CLI.** The CLI does not compile any of them. And a stale CLI
  re-stales fixtures downstream, which is a cold-leaf cascade measured at ~28 s
  per leaf (issue 0604).

## Cause

`cli_source_dirs()` in `packages/cli/nros-cli-core/src/source_stamp.rs` walked
`path = "…"` textually across manifests. It has to be textual: the file is
`include!`d by `build.rs`, so it may not pull in a TOML parser, and it cannot
shell `cargo metadata` — that takes the package-cache lock cargo already holds
during a build.

A textual walk cannot see either of the two things that decide this closure.

**Workspace inheritance.** `nros-orchestration-ir` — which the CLI does
compile — declares:

```toml
nros-rmw = { workspace = true, default-features = false }
```

There is no `path =` on that line. The path lives in the ROOT manifest's
`[workspace.dependencies]`, a file the leaf-manifest scan never reads. So the
walk missed `nros-rmw`, and through it `nros-core`.

**Optional deps.** `nros-board-common` declares:

```toml
nros-platform = { path = "../../platform/nros-platform", optional = true }
```

enabled by feature `deploy-overlay`, which `nros-cli-core` does not enable. The
scan sees a `path =` and follows it — into every platform port, and from there
into `nros-node`, `nros-log`, `nros-smoltcp`, `mps2-an385-pac`, `zpico-alloc`,
`nros-ghost-types` and three generated msg crates.

Measured 2026-08-16, closure vs `cargo metadata`'s resolve for the `nros-cli`
package (non-dev edges):

```
textual walk : 23 dirs outside packages/cli
cargo resolve:  8

under-watched (2): packages/core/nros-core, packages/core/nros-rmw
over-watched (17): mps2-an385-pac, nros-log, nros-node, nros-smoltcp,
                   nros-baremetal-common, nros-platform{,-api,-cffi,
                   -esp32-qemu,-mps2-an385,-stm32f4}, zpico-alloc,
                   nros-zephyr-build, nros-ghost-types, and
                   3 × packages/interfaces/*/generated/humble/*
```

This is the issue-0196 shape twice over in one function, and the under-watch is
a reintroduction of the exact defect the function exists to prevent
(phase-330 W1.a; issue 0561 is the same failure for the play_launch pin).

## How it was found

While attributing issue 0604's cold-leaf cascades. A commit touching only
`nros-node/src/executor/spin.rs` — a diagnostic sink swap the CLI cannot
observe — reported the CLI stale. Reconstructing the walk in Python and diffing
it against `cargo metadata` produced the table above; the under-watch fell out
of the same diff, from the other direction.

## Fix (2026-08-16)

Record cargo's answer instead of reimplementing it.
`scripts/gen-cli-source-dirs.py` runs `cargo metadata` and writes the closure to
the tracked `packages/cli/cli-source-dirs.txt` (a plain newline list — no parser
needed to read it). `cli_source_dirs()` reads that file. `check-cli-source-dirs`
(in `just check`) regenerates and diffs, naming each drift in the direction that
matters:

```
+ <dir>   (the CLI compiles it; the stamp is BLIND to edits there)
- <dir>   (the CLI does not compile it; edits there re-stale the CLI …)
```

The gate is what makes the file safe to trust: a stale list is a silent wrong
stamp in whichever direction it drifted.

The alternative — teaching the walk workspace inheritance and optional-dep
feature resolution — is a cargo resolver reimplementation inside a file that
cannot parse TOML. Cargo computes both exactly and for free.

Two fail-safes, because the failure mode here is silence:

* the list is itself a CLI input (`is_cli_input`), so changing what is watched
  moves the stamp;
* a MISSING list makes `source_stamp()` return `None` rather than fall back to
  `packages/cli` alone. `None` means "cannot tell", which callers already treat
  as "rebuild". Falling back would report FRESH over a silently smaller closure,
  which is the one answer a freshness probe must never give.

### Verified

Against the rebuilt CLI, by editing a file in each class:

| edit | before | after |
| --- | --- | --- |
| `packages/core/nros-node` (CLI never compiles it) | STALE | **FRESH** |
| `packages/core/nros-rmw` (CLI compiles it) | FRESH | **STALE** |
| `packages/core/nros-core` (via `nros-rmw`) | FRESH | **STALE** |
| `packages/cli/nros-cli-core` (control) | STALE | STALE |

The "before" column for `nros-node` is direct observation — that commit is what
started this. The two FRESH rows are closure membership: neither dir appears in
the old walk's output, computed above.

Plus two unit tests, mutation-checked: a listed dir moves the stamp, an unlisted
one does not, and a missing list yields `None`. The old walk had no test for
either direction, because a walk's closure is only checkable against cargo —
which is now the gate's job.

### Residual

`check-cli-source-dirs` runs in `just check`, not in `setup-cli`. The window is
narrow and self-correcting: adding a dep edits a manifest, manifests are
watched, so the stamp moves and the CLI rebuilds correctly — only the WATCHING
of a newly-reachable dir lags, until the next `just check`.

→ issue 0604 carries the remaining cold-leaf attribution (3 rows of ~36 still
over-invalidate on a behaviour-preserving CLI rebuild, by a different path).
