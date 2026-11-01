---
rfc: 0119
title: "CLI extension packages: `runner` and `command` provider kinds, built for the host like any workspace package, with nano-ros's own runners and commands following the same convention"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-527, phase-528, phase-529]
amends: [0087, 0097]
supersedes: []
superseded-by: null
---

# RFC-0119 — CLI extension packages

**Status:** Draft (2026-11-01). Written from the maintainer discussion of
2026-11-01 on how a user adds support for a new board, a new emulator or a new
flasher, and how the result is distributed. Siblings:
[RFC-0117](0117-cli-command-tiers.md) (the command tree) and
[RFC-0118](0118-target-execution-model.md) (what a runner is asked to do).
Implemented by [phase-527](../roadmap/phase-527-cli-extension-packages.md)
(the kinds, the build role, dispatch, the first command packages), with the
first-party runners in [phase-528](../roadmap/phase-528-run-on-host-and-emulator.md)
and [phase-529](../roadmap/phase-529-flash-monitor-bench.md).

## Summary

RFC-0087's provider kinds gain two members: `runner` (drives a launcher,
flasher or console for RFC-0118's verbs) and `command` (adds a verb to
`nros`). A package that provides one is an ordinary package. It has a
`package.xml` announcement and a small descriptor, lives in a workspace or on
the package search path, and is built by `nros build` with everything else.
The difference is that it is built for the host, which follows from its kind.
nros invokes it as an executable through a small versioned contract.
**nano-ros's own runners and commands are packages of exactly this shape**,
under `packages/runners/` and beside the domain they serve. They ship prebuilt
in the toolchain release, and nros has no built-in runner and no in-process
shortcut a user's package lacks.

## Problem

### Adding support means editing nros

A new board is already a package (`nros board new --out-of-tree`, RFC-0064
R5 D1). A new way of running one is not. QEMU invocation lives in
`nros-qemu-ladder` and the descriptor's `runner` string, the FVP is a special
case in `build_next.rs`, the ESP32 launcher lives in `nros-tests`, and a
Renode or Wokwi twin (phase-524) has nowhere to go but another special case.
A user with a vendor flasher can only fork the tree.

### First-party code would be privileged

If our runners were compiled into `nros`, they would differ from a user's in
discovery, in build, in what they may call and in how they are tested. Every
gap between the two is a place where a user's runner fails and ours does not,
found by the user. The maintainers' rule for this RFC: **our runners and
commands follow the same convention users do.**

### Prior art

- **ROS 2:** `ros2 bag` is not part of ros2cli. The `ros2bag` package
  registers a `ros2cli.command` entry point and is built by colcon with the
  rest of the workspace. The verb appears once the workspace is built. A
  package adds CLI capability by being built.
- **Zephyr modules:** a module's `zephyr/module.yml` can declare
  `west-commands` (new `west` verbs) and runner files (new flash and debug
  runners); the module arrives with `west update` like any other project.
  (Recalled; the exact keys are re-checked in phase-527 W1.)
- **cargo and git:** an executable named `cargo-foo` or `git-foo` on `PATH`
  becomes a subcommand. This is cheap, but discovery is `PATH`, so which
  binary runs depends on the shell.
- **colcon:** extensions are Python entry points installed by pip, outside
  the workspace.

## Design

### D1 — Two new kinds

```xml
<!-- packages/runners/nros_runner_qemu/package.xml -->
<export>
  <build_type>nros_cargo</build_type>
  <nano_ros_provides kind="runner" name="qemu"/>
</export>
```

```xml
<!-- packages/rmw/zenoh/nros_zenoh_tools/package.xml -->
<export>
  <build_type>nros_cargo</build_type>
  <nano_ros_provides kind="command" name="router"/>
</export>
```

This amends RFC-0087 D3's kind list: `rmw`, `board`, `platform`, `serdes`,
`runner`, `command`. The tags, the parser and the comment-stripping rules are
unchanged, so the new kinds cost no new parser.

`nros_cmake` is allowed as the build type too. A runner written in C++ or
wrapping a vendor C library builds through the CMake road.

### D2 — Descriptors

Per RFC-0087 D4, a descriptor holds only what cannot be derived.

```toml
# nros-runner.toml
protocol = 1
verbs    = ["run", "console"]     # any of run, flash, console
needs    = ["elf", "nic"]         # plan placeholders it consumes (RFC-0118 D5)
bin      = "nros-runner-qemu"     # default: the package's single bin target
```

```toml
# nros-command.toml
protocol = 1
summary  = "Print or deploy the zenoh router configuration for a bringup"
tier     = "user"                 # user | dev  (dev places it under `nros dev`)
bin      = "nros-zenoh-tools"
```

The verb name is the announced `name`. The cargo feature, crate and C token
derivations of RFC-0087 D4 do not apply to these kinds, because nothing links
against them.

### D3 — The build role follows from the kind

A `runner` or `command` package is a **host tool**. `nros build` compiles it
for the host triple into `<ws>/build/nros/host/<package>/`, whatever board the
images in the same workspace target. Nothing in the descriptor says so.

All the kinds one package announces must agree on host or target. A board
vendor ships a board package and a runner package, two packages in one
repository, as ROS repositories hold several packages. `check-provider-announcements`
refuses a package that mixes them.

Build order uses ordinary dependency edges:

- An image depends on the package providing the runner its plan resolves to
  (RFC-0118 D3), so `nros build --packages-up-to <bringup>` builds the runner.
- `nros run` builds the image if stale (RFC-0116 D7), and that build covers
  the runner package.
- A command package builds with the workspace like any other package.
  `nros <verb>` with an unbuilt command package refuses and names
  `nros build --packages-select <package>`.

### D4 — Where packages come from, and which one wins

Discovery is RFC-0087 D1 and D6, unchanged: a directory with a `package.xml`,
searched over the ordered roots. **D6's precedence is kept.** The nano-ros
packages come first. A user package announcing the same kind and name is
shadowed, and `nros dev ws providers` and `nros doctor` report the shadow. To
replace one of our runners, a user copies it under a new name (`qemu-acme`)
and selects that name at any level of RFC-0118 D5. Which `qemu` ran then never
depends on the search order.

What "the nano-ros packages" means depends on how nros was installed:

| Install | The nano-ros root holds | Built by |
| --- | --- | --- |
| a checkout | `packages/runners/*`, and command packages beside their domain (`packages/rmw/zenoh/nros_zenoh_tools`) | `just setup-cli` (the runners join the CLI build) |
| a released toolchain | `toolchains/<v>/share/nros/packages/<package>/`: `package.xml`, descriptor, `bin/` | the release job (RFC-0097), prebuilt |

A prebuilt package is still found by the same rule. Its `bin/` already
exists, so it needs no build. This amends RFC-0097's release composition: the
toolchain artifact carries the first-party runner and command packages. A
user's packages in `src/` or on `package_paths` are built by `nros build`.
That is the ROS underlay-and-overlay relation, kept at source time.

Execution resolves the executable by absolute path from the package, never
from `PATH`, which is the rule issue 0285 set for `nros-launch-resolve`.

### D5 — Where our packages live

- **Runners** live in `packages/runners/`, one package per method, as their
  own cargo workspace with its own lock (as `packages/cli` is), because they
  are host tools and must not share a target directory with an image root
  (issue 0616).
- **Commands** live beside the domain they serve, as a ROS tool package sits
  beside its library: `nros_zenoh_tools` under `packages/rmw/zenoh/`,
  `nros_px4_tools` beside the XRCE backend, whose PX4 companion path it
  serves (RFC-0039 Track B).
- **The SDK crate** `nros-extension` lives in `packages/runners/`. Runners,
  commands and the core CLI depend on it. It holds:
  - the RunPlan types and their JSON form;
  - the `describe` builder;
  - the console pump and the `link` line;
  - USB VID:PID and port lookup;
  - the exit codes.

  It is published to crates.io, so an out-of-tree Rust runner uses the same
  types. A runner in another language reads and writes the JSON.

### D6 — The runner contract, protocol 1

```
<bin> describe
    → stdout: {"protocol": 1, "verbs": [...], "needs": [...]}

<bin> run|flash|console --plan <plan.json>
    stdout  the target's console stream, unmodified
    stderr  diagnostics; once known, one line `nros-runner: link <endpoint>`
            (for example the pty a serial console was given)
    stdin   console input, forwarded (`nros monitor --send`)
    exit    0 success · 1 the target failed · 2 the runner failed
```

nros owns everything that must behave the same for every runner:

- `--expect` and `--timeout` grading over the runner's stdout;
- router start from the reported link;
- `prepare` steps and `--no-prompt`;
- console capture to `console.log`;
- the process group, and stopping it;
- `--dry-run` and `--why`.

A runner is asked one thing: start this, or flash this, and stream what it
says. The `command` runner that executes RFC-0118 D5's templates is a
first-party runner package too, not a special case.

`protocol` is versioned from the first release. A runner states the
versions it speaks, and nros refuses a runner whose protocol it does not
speak, naming both versions.

### D7 — The command contract

- `nros <verb> <args…>` with no built-in match looks up an announced
  `command` named `<verb>` and executes its `bin` with `<args…>` unchanged.
- The environment carries `NROS_WS_ROOT`, `NROS_TOOLCHAIN` (the version) and
  `NROS_CLI` (the absolute path of the running `nros`). A command asks
  `"$NROS_CLI" dev …` for facts instead of re-deriving them.
- Exit status passes through.
- A built-in or reserved verb cannot be provided. Announcing one is a refusal
  at discovery, not a shadow.
- `tier = "dev"` places the verb under `nros dev <verb>`.
- `nros help --all` lists extension verbs with their `summary` and package.

Dispatch happens in the toolchain's `nros`, not in the launcher
(RFC-0097 D4). The launcher only selects the toolchain.

### D8 — First-party packages

Runners, in the order the phases build them:

| Package | Name | Verbs | Covers | Replaces |
| --- | --- | --- | --- | --- |
| `nros_runner_native` | `native` | run | Linux/POSIX native, `threadx-linux`, `freertos-posix`, Zephyr `native_sim` | ad-hoc spawns in `nros-tests` |
| `nros_runner_qemu` | `qemu` | run, console | every QEMU board, including NuttX and ThreadX on QEMU and ESP32 on QEMU | descriptor `runner = "qemu-system-…"` strings, `nros-qemu-ladder`'s run line, `nros ws qemu-nic`, `nros_tests::qemu`'s command builder |
| `nros_runner_fvp` | `fvp` | run | Arm FVP boards | the FVP special case in `build_next.rs` |
| `nros_runner_west` | `west` | run, flash | Zephyr: `west build -t run`, `west flash` with `--runner` passed through | `west_capabilities`'s reading of `runners.yaml` |
| `nros_runner_command` | `command` | run, flash | RFC-0118 D5 command templates | — |
| `nros_runner_espflash` | `espflash` | flash, console | ESP32 hardware (esp-hal) | the ESP32 flashing path in `nros-tests` |
| `nros_runner_serial` | `serial` | console | the default console for `monitor` and `flash --monitor` | — |

A runner package is written when an in-tree board needs it, not before. The
first is `renode`, for the Nucleo-F767ZI twin phase-524 W1 kept. That board
is a Zephyr board, so its hardware flash goes through `west`, and no probe
runner (`probe-rs`, `openocd`, `picotool`/UF2, `dfu-util`) is written until a
kept board needs one. That is RFC-0087 D5's rule: a package nothing uses is a
fixture, not a proof.

Commands:

| Package | Verb | Was |
| --- | --- | --- |
| `nros_zenoh_tools` | `nros router config` | `nros ws router-config` |
| `nros_px4_tools` | `nros px4 msgs` | `nros generate-px4-msgs` |

### D9 — Trust

A workspace runner or command runs when `nros run` or `nros <verb>` reaches
it. That is the same trust the user already gives the package's `build.rs` or
CMake code when they build it, so this adds no new boundary.

### D10 — Checks

- `check-provider-announcements` covers the two kinds. It refuses a host and
  target mix, a command named like a built-in verb, and a stated descriptor
  field that differs from its derived value.
- `nros dev runner check <name>` and `nros dev command check <name>` build the
  package, call `describe`, check `protocol`, and resolve a dry-run plan.
- `check-board-runner-resolves`: every in-tree board's `[board.run]` and
  `[board.flash]` resolve to a runner package that exists and states every
  placeholder the board's facts need.

## Alternatives considered

- **Executables on `PATH` (cargo, git).** This was the first proposal, and
  it was rejected. It adds a second discovery rule beside the package. Which
  binary runs depends on the shell. And a runner on `PATH` has no version
  relation to the workspace that uses it.
- **Python entry points (ros2cli, colcon).** RFC-0087 D8 rules out a Python
  plugin ABI. An executable contract allows a runner in any language.
- **Built-in runners compiled into `nros`.** That would make ours privileged
  (Problem). The maintainers chose the user convention for ours too.
- **Dynamic libraries loaded by `nros`.** A Rust ABI across separately built
  binaries is not stable, and a C ABI pushes every runner author into FFI for
  what is a process launch.
- **One package carrying a board and its runner.** A package then has two
  build roles (target and host), and the build cannot give it one target
  directory. Two packages in one repository cost nothing.

## Open questions

1. Shell completions for extension verbs: does `describe` for a command
   return its argument shape, or does `nros completions` stop at the verb
   name?
2. Windows: the contract is process-based and portable. Does `bin` need an
   `.exe` rule, or does discovery append it?
3. Does a released toolchain's prebuilt runner need its own version beside
   the toolchain's, for a user who pins a toolchain but wants a newer QEMU
   runner? This RFC says no: a runner ships with the toolchain that speaks its
   protocol.

## Changelog

- 2026-11 — created, from the maintainer discussion of 2026-11-01. Amends
  RFC-0087 D3 (two kinds) and D8 (extension through announced executables),
  and RFC-0097 (the toolchain carries the first-party extension packages).
