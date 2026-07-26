---
id: 285
title: "`nros ws sync` calls `play_launch resolve`, a subcommand the installed play_launch does not have — every platform's build-examples fails"
status: open
type: bug
severity: high
area: cli, build
related: [issue-0197]
---

## Symptom

Any fixture build that runs `nros ws sync` dies at cmake-configure time:

```
error: unrecognized subcommand 'resolve'
Usage: play_launch <COMMAND>
Location:
    nros-cli-core/src/cmd/ws.rs:413:17
error: recipe `build-examples` failed with exit code 1
```

Reproduced 2026-07-26 on `just native build-fixtures` and `just freertos
build-fixtures`; the failing step is shared, so it blocks **build-examples on
every platform**, and `build-workspace-fixtures` with it. Standalone C/C++
example fixtures can still be built by invoking
`scripts/build/fixtures-build.sh <plat> <lang> <rmw>` directly with the
recipe's `NROS_CMAKE_EXTRA_DEFS` (toolchain + `_NANO_ROS_CODEGEN_TOOL`), which
is how the freertos C trio was validated for issue 0268.

## Cause

Version skew between two independently-versioned tools:

- The in-tree CLI (`packages/cli`, rebuilt via `just setup-cli`) invokes
  `play_launch resolve` from `nros-cli-core/src/cmd/ws.rs:413`.
- The `play_launch` binary on PATH resolves to `~/.local/bin/play_launch`
  (commands: `launch`, `run`, `dump`, …) and has no `resolve` subcommand.

`activate.sh` only prepends `~/.nros/sdk/play_launch_parser/bin` (which
provides the differently-named `play_launch_parser`), so the stale
`~/.local/bin/play_launch` wins for the `play_launch` name — the same
"stale binary shadows the activate.sh CLI" class as the `find_program` HINTS
pitfall in CLAUDE.md.

The CLI change that introduced the `resolve` call landed today in the
phase-303/307 line of work; the corresponding `play_launch` release was never
pinned or installed, so the tree is only buildable on a machine that happens
to have a new enough copy.

## What it needs

1. Pin the `play_launch` version the CLI requires (the way
   `PLAY_LAUNCH_PARSER_VERSION` pins `play_launch_parser` in
   `just/workspace.just`), and install it under `~/.nros/…` so `activate.sh`
   controls the PATH entry rather than `~/.local/bin`.
2. Fail loudly and early: `nros ws sync` should probe `play_launch resolve
   --help` and emit a `[PREREQ]` message naming the required version, the same
   way `nros_require_ws_sync` guards the shipped-0.3.7 case (issue #197)
   instead of surfacing a clap error from inside a cmake configure.

Owned by whoever landed the `resolve` call — filed from an unrelated session
(0268 build-graph work) that hit it as a blocker.

## Partial mitigation landed (2026-07-26)

Item 2 is done, in a stronger form than "fail loudly": `resolve_system_models`
now probes `play_launch resolve --help` — the CAPABILITY, not the name — and a
`play_launch` that lacks the subcommand is treated exactly like an ABSENT one,
which was already the designed behaviour (committed `SystemModel`s are used
as-is, with a loud one-line warning so staleness is never mistaken for
freshness). `nros-cli-core/src/cmd/ws.rs`.

Probing `--version` was the actual defect. `play_launch` is also the name of an
unrelated ROS 2 record/replay tool, which answers `--version` happily; the
probe therefore reported "present" and the clap error surfaced from inside a
cmake configure. Degrading is strictly better than failing here: the model is
checked in and usually current, so a host with the wrong tool can still build
everything.

With that, `just build-test-fixtures` proceeds on a host whose `play_launch` is
the wrong tool.

## Still open

Item 1 — pinning and installing a resolve-capable `play_launch` under
`~/.nros/…` so `activate.sh` owns the PATH entry. Until then, committed models
are never refreshed on such a host, so a launch-file edit silently does not
reach the bake.

**Design note (2026-07-26):** rather than pinning an external binary, the
preferred direction is to split `play_launch` — link the Python-free resolve
pipeline into our own toolchain as a library, and keep only the CPython-linked
launch-file parsing in a separately-built tool (built on the user's machine
against the user's Python, the way the CLI is built). That removes the version
skew this issue is about instead of managing it. Being explored; see the
phase/RFC that comes out of it.
