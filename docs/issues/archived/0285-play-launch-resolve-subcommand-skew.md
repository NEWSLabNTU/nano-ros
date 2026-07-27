---
id: 285
title: "`nros ws sync` calls `play_launch resolve`, a subcommand the installed play_launch does not have — every platform's build-examples fails"
status: resolved
type: bug
severity: high
area: cli, build
related: [issue-0197, rfc-0059]
resolved_in: "issue-0285 (nros-launch-resolve helper)"
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

**Design direction (2026-07-26): [RFC-0059](../design/0059-launch-toolchain-split.md).**
Rather than pinning an external binary, split the toolchain — link the
Python-free resolve pipeline into our own tree, and keep only the
CPython-requiring stages in a tool built on the user's machine against the
user's Python. The seam becomes a committable IR artifact, not a CLI verb,
which removes this version skew instead of managing it.

Measured while writing that RFC: all **101** tracked launch files under
`examples/` are XML, **none** uses `$(eval …)`, and the only substitutions in
the tree are `$(var …)` and `$(env …)`. The entire corpus needs no interpreter,
yet is blocked today behind a tool that embeds one unconditionally — which is
why pinning that tool is the weaker answer.

## Resolution (2026-07-27)

Item 1 is done, and not by pinning an external binary — by shipping our own.

### What changed

- **`packages/cli/third-party/play_launch`** — the resolve pipeline is now a
  pinned submodule, versioned with this repo instead of whatever happens to be
  installed. Version skew is structurally gone.
- **`packages/cli/nros-launch-resolve`** — a small, DISTINCTLY NAMED binary
  over that pipeline. It is its own cargo workspace, because it links
  play_launch whose vendored `ros-launch-manifest` would otherwise collide
  with nano-ros's copy in a single cargo graph.
- **`just setup-launch-resolve`** builds it, mirroring `setup-cli`.
- **`ws.rs` resolves it from the running `nros` binary's own path** — a
  sibling, or the in-tree workspace target — and **never** `$PATH`.

### Why the name matters in both directions

Renaming is not just about not being shadowed. Had we shipped our build as
`play_launch` on PATH, we would have shadowed the user's real `play_launch`
(the ROS 2 record/replay tool), silently breaking a workflow that has nothing
to do with nano-ros. That is a worse failure than this issue, because it is
silent. Hence: distinct name, absolute-path invocation, and deliberately NOT
added to PATH.

### The Python constraint, unchanged

A separate process is irreducible and always will be: `.launch.py` requires
executing the *user's* CPython, and pyo3's `auto-initialize` would pin
libpython into `nros`, ending its libc-only portability (phase-195.A). What
was removable was the *unpinned, PATH-resolved* dependency — not the process.

Upstream gating (play_launch `8b9ba98` + `d546caf`) makes the helper link with
`default-features = false`, dropping `rclrs` and the colcon-generated message
crates. Those are not registry deps — `colcon-cargo-ros2` generates them from
the ament environment and patches them in, which is why they are pinned `"*"`.
So without that gating, building our helper would have required play_launch's
entire colcon setup, forcing ROS onto embedded users who have neither ROS nor
colcon. Now it needs CPython and nothing else.

### Receipts

- With a hostile `play_launch` on PATH that fails every invocation, `nros sync`
  runs OURS: the surfaced error traces into
  `packages/cli/third-party/play_launch/.../resolve.rs` and is a genuine
  `system.toml` placement error, not a tool-resolution failure.
- `nros-launch-resolve <launch> -o model.yaml` resolves a real launch file with
  no ROS environment and no colcon.
- `just qemu build-fixtures` rc=0 with zero `play_launch` references in the log.
- 62 cli test binaries green; clippy clean.

### Tests

`launch_resolver_tests` in `cmd/ws.rs` gates the core property — resolution
never consults `$PATH`. One test places a valid helper on PATH *only* and
asserts it is NOT found, which is precisely the hijack this issue is about.
Plus the installed (sibling) layout, the in-tree layout, and absent → `None`
so the caller degrades to the committed model.
`native_orchestration_misuse` was asserting the old `play_launch resolve`
string and now asserts the new name.

### Follow-ups (not blocking)

- `nros::main!` diagnostics and the misuse test now name the helper; docs and
  comments that still say "play_launch" describe the upstream project or the
  pipeline's behaviour, not a command to run.
- RFC-0059's remaining half — linking the Python-FREE XML/YAML path directly so
  the helper is only spawned for trees that actually contain `.launch.py` or
  `$(eval …)` (0 of 101 tracked launch files today) — is still worth doing, and
  is now unblocked by the `runtime` feature gate.
