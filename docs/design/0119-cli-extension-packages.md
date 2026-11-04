---
rfc: 0119
title: "CLI extension packages: `runner` and `command` provider kinds, built for the host like any workspace package, with nano-ros's own runners and commands following the same convention"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-527, phase-528, phase-529, phase-533]
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
- **Zephyr modules and west:** a module's `zephyr/module.yml` can list
  runner files (`runners: [{file: scripts/my_runner.py}]`). Each listed file
  is imported when `west flash` or `west debug` runs, and its
  `ZephyrBinaryRunner` subclasses become runners. New `west` verbs are
  declared elsewhere: a project entry in the west manifest names a
  `west-commands:` file (`west-commands.yml` by convention). Either way the
  code arrives with `west update` like any other project. (Checked in
  phase-527 W1 against Zephyr's `scripts/schemas/module-schema.yaml`, which
  has a `runners` key and no `west-commands` key, and against west's
  extension-commands documentation. The first draft of this paragraph put
  both keys in `module.yml`.)
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

How the build does it (phase-527 W2):

- A cargo host tool builds with `--release --target <host triple>` and
  `--target-dir <ws>/build/nros/host/<package>/target`, so a `[build] target`
  in a `.cargo/config.toml` above it cannot make it a cross build. A CMake
  host tool configures its own `CMakeLists.txt` into
  `<ws>/build/nros/host/<package>/cmake`.
- The place is fixed: `--build-base` does not move it, because dispatch and
  `nros run` find the executable there from the workspace alone.
- The image edge is resolved on the provider search path (RFC-0087 D6). Only
  a runner whose winning provider is a package of this workspace adds an
  edge; one provided from the nano-ros root or a released toolchain is
  already built. A dependency on a host tool is exempt from the selection's
  closure check, since leaving one out cannot break the image's root.
- That resolution is a resolution in D4's sense: a workspace runner that
  replaces one of ours without an `[overrides]` entry (or
  `--allow-overriding`) refuses the build, and a declared one gains its edge.
- With no selection, every host tool in the workspace builds. A selection
  that keeps only host tools builds no image.

### D4 — Where packages come from, and which one wins

Discovery is RFC-0087 D1 and D6, unchanged: a directory with a `package.xml`,
searched over the ordered roots. **Precedence is colcon's overlay rule, as D6
already implements it**: a later root overlays an earlier one, and the roots
are, earliest first, the nano-ros root (a checkout or a released toolchain),
the workspace, `[workspace] package_paths`, then `NROS_PACKAGE_PATH`. So a
workspace package announcing the same kind and name as one of ours wins. The
order is fixed and deterministic, and `provider_scan::resolve_unique`
implements it; this RFC changes who may use that win, not who gets it.

*(Amended in phase-527 W2b. The first text said the opposite: our packages
won and the user's copy was shadowed. The code had always let the overlay
win, and phase-527 W2's build edge follows the code, so the RFC now does
too.)*

An override is allowed, and it is never silent:

1. **Declared overrides.** A workspace states which of its packages may
   replace an underlay provider, in `system.toml`:

   ```toml
   [overrides]
   qemu = "src/my_qemu"   # provider name -> the package path allowed to replace it
   ```

   The path is relative to the workspace root, and the declaration is pinned
   to it, not to the name: a different package that later announces `qemu`
   (from `package_paths`, say) does not inherit the approval. A workspace with
   several bringups may declare entries in any of their `system.toml`s; one
   name pinned to two paths is refused.
2. **An undeclared override of a `runner` or `command` is refused**, because
   those kinds run host code and flash hardware, and which package does that
   must never be decided by search order alone. It is refused when the name
   is *resolved*: by `run`/`flash` plan resolution, by command dispatch, or by
   D3's image-to-runner build edge. A scan or a listing never refuses, so a
   stray copy nobody selects never gets in the way. The message is one line
   and the fix:

   ```
   src/my_qemu announces runner `qemu`, which nros_runner_qemu (<path>) also provides.
     To use yours, add to system.toml:   [overrides] qemu = "src/my_qemu"
     or for this run only:              --allow-overriding qemu
   ```
3. **An undeclared override of any other kind** (`rmw`, `board`, `platform`,
   `serdes`) warns once at resolution and proceeds, as colcon does.
4. **`--allow-overriding <name>…`**, colcon's spelling, approves by name for
   one invocation. It is accepted by the verbs that resolve providers: `nros
   build` today, and `run`, `flash` and `monitor` as their runner resolution
   lands (phase-528, phase-529).
5. **A declared override is quiet.** Visibility comes from asking instead:
   `nros dev ws providers` and `nros doctor` list every override (name,
   winner, the path it shadows, declared or not), and `--dry-run --why` on
   `run`/`flash` names the runner that won and why.
6. **Protocol.** An overriding runner or command must speak a protocol
   version the package it replaces speaks (D6; versions are whole numbers, so
   each is a major version). Otherwise it is refused, naming both lists, even
   when declared.
7. **Stale declarations.** An `[overrides]` entry whose path no longer
   announces that name is an error at resolution, never a silent fall back to
   the underlay. An entry that overrides nothing (no other package provides
   the name) is a warning.
8. **Commands.** D7's rule stays: a built-in or reserved verb cannot be
   provided at all. Overriding a first-party command package follows rule 2.

**Prior art: colcon.** Measured with colcon-override-check 0.0.1 on Humble:
the overlay wins at runtime (an `AMENT_PREFIX_PATH` prepend). `colcon build`
prints a WARNING and builds anyway, and only when an underlay package
run-depends on the overridden one; overriding a leaf is silent.
`--allow-overriding <pkg>` silences the warning by name only, and the
extension's source says the warning "may be promoted to an error in a future
release". `colcon list` never warns. We keep the precedence and the flag, and
close the gaps:

| colcon | nano-ros |
| --- | --- |
| warns only when an underlay package depends on the overridden one | every override is found, leaf or not |
| a warning, then the build proceeds | refused for `runner` and `command`; a warning for the other kinds |
| approval by name, so a later package of that name inherits it | `[overrides]` pins a path |
| no committed record of an intended override | `[overrides]` in `system.toml`, reviewed with the workspace |
| `colcon list` says nothing | `nros dev ws providers` and `nros doctor` list every override and whether it was declared |
| an approval for a package that is gone is ignored | a stale entry is an error; an unused one warns |
| no interface check | an overriding runner or command must share a protocol version |

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
- 2026-11 — phase-527 W1: corrected the Zephyr prior-art paragraph
  (`module.yml` carries `runners`; `west-commands` belongs to the west
  manifest). Landed D1, D2 and the D10 refusals in `check-provider-announcements`
  (A6 host/target mix, A7 built-in verb, A8 stated `bin`).
- 2026-11 — phase-527 W2: D3's build role landed in `nros build`; D3 records
  the host build's place, profile and triple, and how the image edge
  resolves.
- 2026-11 — phase-528 W3: D8's `native`, `qemu`, `fvp`, `west` (run) and
  `command` runner packages landed under `packages/runners/`, speaking D6
  protocol 1 unchanged; `qemu` takes its executable from the plan's `tools`
  (filled by nros through the QEMU ladder) and never from `PATH`, and
  implements `run` only until `nros monitor` needs `console`. D10's
  `check-board-runner-resolves` landed for `[board.run]`; its `[board.flash]`
  half waits for the hardware runners (phase-529).
- 2026-11 — phase-527 W2b: D4 rewritten. Precedence is colcon's overlay rule
  (the workspace wins), as the code always did; the first text said nano-ros
  packages won. An override is now declared and path-pinned in `system.toml`
  `[overrides]`; an undeclared `runner` or `command` override is refused at
  resolution, other kinds warn; `--allow-overriding` is a one-off approval;
  an override must share the protocol of the package it replaces; stale
  entries error and unused ones warn. Adds the colcon prior art and the
  comparison table.
