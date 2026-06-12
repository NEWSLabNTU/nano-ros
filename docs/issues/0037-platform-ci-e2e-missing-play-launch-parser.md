---
id: 37
title: platform-ci e2e — `play_launch_parser` not provisioned/PATH'd (build-fixture-extras fails)
status: open
type: bug
area: ci
related: [issue-0034, phase-196, phase-240]
---

> **FIX v2 2026-06-12 (pending re-validation).** The v1 `$GITHUB_PATH` append
> (commit 7b0517121) did **not** work — run 27395089910 still failed: the install
> ran fine (binary at `/github/home/.nros/sdk/play_launch_parser/bin`) but the
> later steps `source ./setup.bash`, which **strips** `~/.nros/sdk/*` from PATH
> (`setup.bash:68`) and only re-adds the `*/*/bin` SDK layout (`setup.bash:56`) —
> play_launch_parser's `*/bin` (two-level) layout is never re-added. So the
> `$GITHUB_PATH` entry was wiped before `nros plan` ran. (activate.sh special-cases
> the `*/bin` layout at lines 73–74; setup.bash does not — the documented
> sweep-contract divergence.) v2 instead exports
> `NROS_PLAY_LAUNCH_PARSER=<abs path>` via `$GITHUB_ENV` — nros's designed
> override (`planner.rs:459`), an env var that survives the PATH strip. e2e-gated;
> targeted (not `source activate.sh`, which would shadow the in-tree `nros`).

The platform-ci **Test/e2e** step fails in `build-fixture-extras` because
`play_launch_parser` is not on PATH. Surfaced by run 27393704883 (threadx_linux
cell):

```
nano_ros_workspace_metadata: `nros plan` failed (rc=1)
stderr: Error: failed to run `play_launch_parser` (launch parser) for ...
error: recipe `build-fixture-extras` failed with exit code 2
```

## Root cause

`play_launch_parser` is a separate binary from `nros`. It is installed by
`just workspace install-play-launch-parser` into
`~/.nros/sdk/play_launch_parser/bin/`, and `activate.sh` (the env/PATH SSoT)
PATHs it from there (`activate.sh` lines 68–74). The platform-ci job:

- builds + PATHs **only** `nros` (`cargo build --bin nros` →
  `packages/cli/target/release` on `$GITHUB_PATH`); it never builds/installs
  `play_launch_parser`, and
- the Build/Test steps `source ./setup.bash`, **not** `./activate.sh` — so even
  if it were installed, the PATH wiring the sweep contract requires is absent.

This is the CLAUDE.md **sweep contract** pitfall: *"every `just <plat>`
invocation needs `source ./activate.sh` first (PATH wires nros,
play_launch_parser, zenohd). The pre-218 `export PATH=...` is insufficient."*
`build-fixture-extras` reaches `nros plan` → shells out to `play_launch_parser`
→ not found.

## Scope

Affects every e2e cell whose fixtures go through `nros plan` /
`build-fixture-extras` (launch-manifest path) — threadx_linux observed; the
others that don't hit `build-fixture-extras` (qemu, esp32) went green. push/PR
runs are unaffected (build-only, no e2e). Disk/build-scope (phase-240) is NOT
the cause — Build steps are green; this is purely the e2e tool-provisioning gap.

## Fix direction

Provision + PATH `play_launch_parser` in the platform-ci e2e path. Either:
- run `just workspace install-play-launch-parser` before the Test step and
  source `./activate.sh` (the SSoT) instead of bare `setup.bash`, or
- add `play_launch_parser` to the in-tree CLI build (build + PATH it alongside
  `nros`, since both live in `packages/cli/`).

Prefer sourcing `activate.sh` — it is the declared PATH SSoT and `just doctor`
enforces it; relying on `setup.bash` re-introduces the pre-218 insufficiency.

Owner: CI / phase-196 follow-up (continuation of phase-240's e2e validation).
