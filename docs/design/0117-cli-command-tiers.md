---
rfc: 0117
title: "CLI command tiers: a task-grouped user tier, a hidden `nros dev` tier, ROS 2 flag names where the meaning is the same, and aliases instead of removals"
status: Draft
area: docs-ux-diagnostics
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-526, phase-568]
amends: [0116]
supersedes: []
superseded-by: null
---

# RFC-0117 — CLI command tiers

> **Amended by [RFC-0120](0120-nros-configure-one-resolver.md) (2026-11-03).** `configure` joins the Build group and absorbs `sync`, which stays as its alias.
>
> **Amendment 2026-11-17:** build arguments (D7–D9), approved, not yet built (phase-568); see the section of that name. Build profiles are deferred to a later amendment.

**Status:** Draft (2026-11-01). Written from the maintainer discussion of
2026-10-31 and 2026-11-01 on how the `nros` command tree should look to a
user and to a contributor. Its two siblings are
[RFC-0118](0118-target-execution-model.md) (how an image is run, flashed and
monitored) and [RFC-0119](0119-cli-extension-packages.md) (packages that add
runners and commands). Implemented by
[phase-526](../roadmap/archived/phase-526-cli-two-tier-restructure.md).

## Summary

`nros` has 33 top-level verbs, and `nros ws` alone has 25 subcommands that mix
what a user types with what cmake, west and `just` call. This RFC splits the
tree in two. The **user tier** is small and grouped by task: start, build,
run on a target, workspace, interfaces, inspect, toolchain. The **dev tier**
lives under `nros dev`, is hidden from default help, and holds every verb
whose caller is a build system, a gate or a contributor. Flags follow one
rule: where a flag means what a ROS 2 or colcon flag means, it takes that
flag's exact name; where the meaning differs, it takes its own name. Nothing
is removed. Every old spelling stays as a hidden alias, and a gate keeps the
tree's own docs and scripts on the new spelling.

## Problem

Measured on `c16e26f0b2` (the full tree is in the phase doc):

- **Two audiences share one namespace.** `nros --help` lists `image-facts`,
  `model-path`, `sdk-path`, `sdk-front`, `sdk-root`, `codegen-system` and
  `metadata` beside `build`, `new` and `run`. Each of those exists so cmake,
  west or a shell recipe can ask the CLI one question; none is something a
  user decides to type.
- **`nros ws` is a drawer.** Its 25 subcommands span the user's own
  workspace (`list`, `clean`, `status`), lock maintenance (`seed-lock`,
  `check-lock`, `managed-deps`), per-image facts for a configure step
  (`leaf-system`, `board-facts`, `qemu-nic`, `sizing-descriptor`), RMW
  lowering (`rmw-dispatch`, `rmw-config`, `session-properties`) and the
  entity census. The prefix says nothing about which group a verb is in.
- **Help text speaks in ids.** 238 help lines across 81 verbs open with a
  phase, issue or RFC id (`phase-440 W6 — …`), because clap renders the `///`
  doc comment and the doc comment is where the history was written. RFC-0116
  D2 already forbids that on the user surface.
- **ROS users meet unfamiliar flags for familiar ideas.** `nros build` selects
  packages, `nros new` scaffolds one, and `nros sync` (now `nros configure`) names a build directory,
  each with flag names a colcon or `ros2 pkg create` user has to learn again.

## Design

### D1 — Two tiers

The **user tier** is what `nros --help` shows. Every verb in it is something
a user decides to do. The **dev tier** is `nros dev <noun> <verb>`, hidden
from `nros --help` and shown by `nros help --all` and `nros dev --help`. The
noun groups callers by the question they ask, so a contributor looking for
"the command cmake runs to get the image's facts" finds it under
`nros dev image`.

Hiding is not removing. A dev verb keeps its behaviour, its exit codes and
its output format, because cmake, west and `just` parse that output.

### D2 — The user tier

> **Amended 2026-11-07 (user ruling, issue 2294):** `check` joins the Inspect
> group as one verb, `nros check <package>`, which works out whether the
> package is a runner, a command or a board and runs that kind's check. It
> replaces the unbuilt `board check`. See
> [Amendment 2026-11-07](#amendment-2026-11-07--one-user-tier-check-for-a-package).
>
> **Amended 2026-11-14 (phase-555 W3, user ruling on the design review):** the
> table records the tree `nros --help` ships, measured from a build of `main`.
> Two verbs left their groups after this table was written. `explain` is a
> top-level Inspect verb; [RFC-0129](0129-diagnostic-codes.md) D5 extends it to
> `nros explain NROS-NNNN`, which a `plan` subcommand could not carry. And
> `update` is a top-level Toolchain verb (RFC-0104 D7). `nros plan <bringup>` takes no `resolve|check|explain`
> subcommands, `check` is top-level (amendment 2026-11-07), `pin` takes an
> optional `[VERSION]`, and `board` gained `show` (a board's support
> evidence).

| Group | Verbs |
| --- | --- |
| Start | `new` · `setup` (absorbs `init`) |
| Build | `build` · `configure` (alias `sync`, RFC-0120) · `clean` · `materialize` |
| Run on a target | `run` · `flash` · `monitor` · `bench` · `debug` (reserved) — RFC-0118 |
| Workspace | `list` · `graph` |
| Interfaces | `interface generate\|list\|show` |
| Inspect | `doctor` · `board list\|show\|info\|new` · `config show\|explain` · `plan <bringup>` · `check` · `explain` |
| Toolchain | `toolchain list\|remove\|gc` (absorbs `store`) · `pin [VERSION]` · `update` · `locate` |
| Other | `version` · `completions` · `help` |
| Extensions | verbs contributed by `command` packages — RFC-0119 |

`nros help --all` lists every verb of both tiers and, for an extension verb,
the package that provides it.

### D3 — The dev tier

| Noun | Holds (today's spelling) |
| --- | --- |
| `dev codegen` | `codegen *`, `generate-rust`, `codegen-system` |
| `dev image` | `image-facts`, `model-path`, `ws leaf-system`, `ws board-facts`, `ws sizing-descriptor`, `ws west-leaf-sizing`, `ws entity-inventory`, `ws entity-census` |
| `dev rmw` | `ws rmw-dispatch`, `ws rmw-config`, `ws session-properties` |
| `dev lock` | `ws seed-lock`, `ws check-lock`, `ws managed-deps` |
| `dev profile` | `profile *` |
| `dev board` | `board cmake-vars`, `board net` |
| `dev ws` | `ws env`, `ws order`, `ws providers` |
| `dev sdk` | `sdk-front`, `sdk-path`, `sdk-root`, `toolchain manifest` |
| `dev metadata` | `metadata` |
| `dev runner` / `dev command` | new: `check` for an extension package — RFC-0119; since 2026-11-07 the silent alias of `nros check <package>` (Amendment 2026-11-07) |

Three of today's verbs leave the core binary rather than moving within it:
`ws qemu-nic` is absorbed by the QEMU runner package (RFC-0118), and
`ws router-config` and `generate-px4-msgs` become command packages
(RFC-0119 D7).

### D4 — The ROS flag rule

**Same meaning, same name.** Where an `nros` flag means exactly what a
colcon, `ros2 pkg create`, `ros2 interface` or `ros2 doctor` flag means, it
takes that flag's name, spelling included.

**Different meaning, own name.** Where the meaning differs, the flag keeps a
name of its own, even when the ROS name is close. A familiar name with a
different meaning is worse than an unfamiliar one, because the reader trusts
it.

Applied:

| Verb | Flags taken from ROS 2 / colcon | Kept as our own, and why |
| --- | --- | --- |
| `build` | `--packages-select`, `--packages-up-to`, `--packages-skip`, `--packages-above`, `--continue-on-error`, `--cmake-args`, `--base-paths`, `--build-base` | `-j/--jobs` instead of `--parallel-workers` (see below). Otherwise the same names; the divergences below are refusals or a unit, never a different answer |
| `sync` | `--base-paths` | `--generated-base` (was `--build-dir`): not colcon's `--build-base` — see below |
| `list` | `-t` (topological), `-n` (names only), `-p` (paths only), as `colcon list`, plus `build`'s selection flags and `--base-paths` | — |
| `graph` | as `colcon graph` | — |
| `new` | `--dependencies`, `--node-name`, `--license`, `--maintainer-name`, `--maintainer-email`, `--description`, `--destination-directory` | `--lang` (not `--build-type`: our package's build type is derived from the language and the board, RFC-0087 D2, so the ROS flag would name a choice the user does not make); `--component` (not `--library-name`: `ros2 pkg create` adds a library target beside the node, while a nano-ros library package is a component whose library takes the package's name — crate name, exported symbol and linkage are all derived — so there is no second name to give); `--board`, `--template`, `--kind` (no ROS counterpart). `--build-type` and `--library-name` are parsed, hidden, and refused naming the flag to use |
| `interface` | `list`, `show` as `ros2 interface` | `generate` (no ROS counterpart) |
| `doctor` | `--report` as `ros2 doctor` | — |
| `run` | `--prefix` as `ros2 run`; arguments after `--` | `--expect`, `--timeout`, `--router` (no ROS counterpart) |

A flag with no ROS counterpart follows the existing `nros` spelling.

Where `build` keeps a colcon name but cannot give colcon's whole answer, it
refuses rather than answering differently, or names the unit it acts on
(phase-526 W3 measured each against `colcon list` over
`packages/cli/testing_workspaces/ros2_rust_examples`):

| Flag | Where it differs from colcon, and why |
| --- | --- |
| every selection flag | A name matching no package is an error, not a warning, and a selection that leaves out a package a selected one `<depend>`s on is refused (colcon lists it and builds it against the install prefix). nano-ros has no install prefix (RFC-0087 D8): a package left out is absent from the one merged root. For `--packages-above` this means a named package with workspace dependencies needs `--packages-up-to` beside it. |
| `--packages-above` | Refused for a cargo member with no `package.xml`: its dependents name it only as a `Cargo.toml` `path` dependency, which the `<depend>` graph does not hold, so the answer would silently be smaller than colcon's. |
| `--continue-on-error` | In a workspace with no bringup the unit is the package, as in colcon: a failed package's dependents are skipped. In a workspace with images the unit is the IMAGE — each is one merged build, so a failure inside it fails that image, and the next image is still attempted. |
| `--parallel-workers` → `-j/--jobs` | NOT taken. colcon counts PACKAGES there (default: the CPU count) because it has no shared job pool: each CMake package gets its own `-j<cpus> -l<cpus>` (`colcon_cmake`), so N packages at N jobs is N² processes, held back only by make's load-average limit, which cargo ignores. `nros build` has one GNU make jobserver for everything it runs, so the knob that loads the machine is the JOB count, and it is spelled as make, ninja and cargo spell it: `-j N` / `--jobs N`, default the CPU count (cargo's default too). Packages start once their dependencies finish and hold a token while they run; images build one at a time, each on the same pool, so `-j` applies to images and packages alike. `--parallel-workers` is parsed, hidden, and REFUSED naming `nros build -j <N>`: accepted with colcon's meaning it would add a second knob that the pool makes redundant, and accepted as the job count it would silently change a pasted colcon command (`--parallel-workers 1` keeps full-speed compiles there, and would be a serial compile here). The pool: `MAKEFLAGS` / `CARGO_MAKEFLAGS` to every child, `CMAKE_BUILD_PARALLEL_LEVEL` removed so `cmake --build` passes no `-j` that would bypass it; under an outer `make -jN` that already provides a jobserver, `nros build` joins it. The FIFO form (`--jobserver-auth=fifo:PATH`) is the only one ninja reads; the pinned ninja 1.13.2 does (measured: 8 one-second edges under a 2-token pool took 4.0 s, and 1.0 s with an explicit `-j8`, which ninja lets override the pool). Make 4.3 is FATAL on the FIFO form, so when the `make` on PATH is older than 4.4 the pipe form is used, which every make and cargo read and ninja does not. That is asked of PATH whatever the driver or CMake generator, because a Ninja or west build reaches make too: Zephyr's native_sim links `zephyr.exe` through the native simulator's Makefile, and inheriting the FIFO pool killed that link (`invalid --jobserver-auth string`, issue 2219). A ninja that cannot read the pool (older than 1.13, or a pipe-form pool) runs at its own width, and `nros build` prints so rather than oversubscribing silently. |
| `--build-base` | `<DIR>/<package>/` in a workspace with no bringup, as in colcon. Refused for images: an image's generated tree has a fixed place under `build/` that `nros dev image facts`, the cmake module and the fixtures read. |
| `--cmake-args` | Reaches every CMake configure, including a west image's (west's second zone); a cargo image has none, as colcon ignores the flag for a non-CMake package. |
| `--base-paths` | ONE meaning on every verb that takes it — `build`, `list`, `sync`, `dev ws providers`, and `run`/`flash`/`monitor`, which hand it to their implicit build and runner lookup: the workspace's source roots, colcon's base paths (RFC-0119 D4). Default `.` plus `nros.toml` `package_paths`. It replaces those and nothing else; the nano-ros underlay and `NROS_PACKAGE_PATH` stay on the provider search path (colcon's underlay is `AMENT_PREFIX_PATH`, which the flag does not touch either). The roots are peers, order carries no precedence, a path repeated or inside another counts once, a package name in two of them is an error naming both, a missing path is an error and an empty one a warning. Relative paths are taken against the workspace (`--workspace`, else the cwd), where colcon takes them against the cwd; the two agree whenever `--workspace` is left out. |

`sync`'s output directory is `--generated-base`, not `--build-base`. It
names only the generated message crates (default `generated`); the resolved
models and the rest of `sync`'s output stay under `build/`. colcon's
`--build-base` moves all build output, and `nros build --build-base` names the
package build directories, so a third meaning under the same name would put
message crates and package builds side by side in one directory while the
models stayed behind. `--build-dir`, the flag's old name, is a hidden alias.

### D5 — Aliases, never removals

Every old spelling resolves, hidden from help:

- An **old user-tier spelling** (`nros setup presets`, `nros toolchain gc`,
  `nros list`) works and prints one line on stderr naming the new
  spelling. The line is suppressed when stderr is not a terminal, so a script
  that captures output sees no change.
- An **old dev spelling** (`nros dev image facts`, `nros dev image board-facts`) works
  silently. Its callers are cmake, west and `just`, inside the tree and in
  users' trees built against an older toolchain, and a warning there is
  noise in a build log nobody reads.
- An old flag spelling (`--build-dir`) is a hidden alias of the new flag.

An alias is retired only by a later RFC amendment that names the release it
leaves in, because an out-of-tree cmake file can call it.

**Gate:** `check-cli-old-spellings`, on the fast line, refuses an old
spelling in the tree's own `cmake/`, `just/`, `scripts/`, `zephyr/`,
`book/src/` and `docs/` (outside RFC and phase docs that quote it). The tree
moves to the new spelling in the same phase that adds it; users' trees move
when they choose.

### D6 — Help text carries no internal id

This is RFC-0116 D2 applied to the source: the history a `///` doc comment
carries moves into a `//` comment above it, so clap renders what the verb
does and the contributor keeps the history beside the code. phase-525 W1's
`check-user-surface-ids` is the gate, and runs after this tree has landed so
the sweep happens once, over the final verb names.

## Amendment 2026-11-07 — one user-tier check for a package

User ruling, 2026-11-07 (issue 2294). D2 gave a user no way to check a
package they wrote. The runner and command checks sat in the dev tier (D3),
`board check` was listed and never built, and the scaffolds therefore named
`nros dev … check` and `just check fast` as their next step.

- **One verb.** `nros check <package>` is the user-tier check for a package
  of any kind. It finds the package through the provider scan and the board
  catalog a build uses, by the name it announces or by its package name. A
  runner gets the RFC-0119 D10 runner check, a command the command check and
  a board the board check below. A package of two kinds gets both checks.
  Per-kind verbs (`nros runner check`, `nros command check`, `nros board
  check`) were the other option and were not chosen: they add three nouns to
  the user tier, and a user would have to know the kind before checking.
- **The workspace forms stay.** `nros check` already took a path (a plan, a
  `system.toml`, a bringup directory) and `--workspace`. A positional that
  exists on disk, or that holds a character no package name holds (`/`,
  `.`), is a path as before. Only a bare name (`[A-Za-z0-9_-]`) with nothing
  of that name on disk is a package. No path form could read such a name, so
  no command that worked before changes meaning. `--workspace` keeps its
  meaning; the package form takes the workspace from the current directory,
  as `nros build` does.
- **The board check** is the validation that already existed, run for one
  board: the descriptor resolves through the catalog (parsed, and announced
  by its `package.xml`, RFC-0064 R5 D5); the announced names equal the
  declared ones (`check-provider-announcements` A2, which only sees the
  tree); `[board.net]` resolves; a `[board.zephyr]` board projects into the
  cmake variables `nros board info` prints.
- **Aliases.** `nros dev runner check <name>` and `nros dev command check
  <name>` stay as silent dev aliases (D5). The scaffolds name
  `nros check <name>`; `nros board new` keeps `just check fast` on its
  in-tree arm only, after `nros check`, as the contributor's acceptance.

## Amendment: user-tier verbs for 2394

> **Decided 2026-11-10 (user ruling); implemented 2026-11-10 (issue 2394,
> resolved).** The table and the four answers below are decisions, and every
> site in the table now gives its user-tier advice.

Issue 2394 lists the user-facing sites that still tell a user to run a dev
verb (D3), because the action they ask for had no user-tier verb. Each
carried a `dev-verb-advice-ok` marker naming that issue, which
`check-user-text-dev-verbs` accepts only while the issue is open.

**Principle.** Fold an action into a verb the user already runs before
adding one: a new flag beats a new verb, and a new verb beats a dev verb.
Advice names a command the book teaches, run from where the user already
is, usually the workspace root.

| Site (who reads it) | Today | Home | Advice |
| --- | --- | --- | --- |
| `nros new` component, "Next steps" | `nros dev metadata --build` | `nros build` records a component's source metadata when it is missing or stale | `nros build   # builds it and records its source metadata` |
| `nros configure`, stale census | `nros dev image entity-census {how}` | a flag, `nros configure --census`: build the native image, run it, record the census. Plain `configure` keeps today's cost and only reports | "…the entity census for `<image>` is STALE; take a new one with `nros configure --census` (it builds and runs the native image, so it takes longer)" |
| a cmake configure's census refusal | `nros dev image entity-census take --image B:I` | the same flag, for one image: `nros configure --census B:I` | "…in the workspace, run `nros configure --census B:I`, then build again" |
| census verdict rows: an unobserved node or parameter | `nros dev image entity-census run` | the same flag | "…the census never ran `<node>`; run `nros configure --census` with an image that launches it, or …" (the row's existing accept wording) |
| `nros configure`: `system.toml` beside code | `nros dev migrate-bringup` | a flag, `nros configure --migrate`: applies known, reversible migrations and prints every file it moves. `nros doctor` stays read-only and names the flag | "…`system.toml` belongs in a bringup package; `nros configure --migrate` moves it and says what it moved" |
| the PX4 bridge example: FFI archive missing outside a workspace | `nros dev lock central-patch` | no new verb: inside a workspace `nros configure` writes the patch; outside one, the user path is the PX4 module template | "…build this module from an nros workspace (`nros configure` writes the patch it needs), or start from the module template `nros locate nano-ros` points to" |

**Answers (decided):**

1. *Does recording source metadata need the native build the census needs?*
   If it does, it goes under `--census`, so plain `nros build` stays cheap,
   and "Next steps" names `nros configure --census`. If it does not,
   `nros build` records it.
2. *`--census` on `configure` or on `build`?* On `configure`, where the
   census is consumed (it feeds sizing, which `configure` resolves).
3. *May `doctor` write files?* No. `doctor` stays read-only, and its
   finding names `nros configure --migrate`, which does the move and prints
   every file it moves.
4. *Book wording.* The user pages that mention the census and the
   bringup move change in the same PR as the CLI change, not before.

Done when each site is reworded to its user-tier home and no site carries
the issue's marker. Answer 1 resolved the second way: the source metadata is
refreshed by configure's first phase, which `nros build` runs, and that
needs no native image, so `nros new` names `nros build`.

## Amendment 2026-11-17 (approved, not yet built) — build arguments

Approved by the user on 2026-11-17, as revised after review (profiles
deferred). Not yet built. Implemented by
[phase-568](../roadmap/phase-568-build-args-and-profiles.md). D4's
`--cmake-args` row and the `--` passthrough change as described under
"Migration".

### Motivation

The user raised two concerns:

1. **colcon argument confusion.** `--cmake-args` today is colcon's flag: it
   takes one or more values, hyphens allowed, and ends "at the next `nros
   build` flag or at `--`". So `--cmake-args -DFOO=1 --packages-skip x` reads
   one way to a colcon user and another to a reader who does not know where
   the list ends. A value that happens to start like a build flag cannot be
   passed at all.
2. **Plugin interpretation and injection.** Arguments for the native tools
   reach several routes: a CMake image, west's two argument zones, cargo, a
   cmake package. If each route (or a plugin) interpreted them its own way,
   one argument could mean different things on different roads, and a project
   or a dependency could re-wire a build silently.

Today `nros build` has three channels for this, each with its own rules:
- `--cmake-args`, colcon-style;
- everything after `--` (`native_args`), handed to the native tool verbatim
  and, on the west road, split by west's own flag list;
- `[image.<id>] profile` in `system.toml`, mapped to cargo's `--profile` and
  to `CMAKE_BUILD_TYPE` through `nros_cargo_profile::cmake_build_type`, with
  a board default (`board_cargo_profile`: `release` except on `posix`).

A `--profile` or `--release` after `--` wins over the stated profile, and a
`-DCMAKE_BUILD_TYPE` after `--` wins on the cmake road.

### D7 — Where arguments come from (in order; a later one wins)

1. **The image's own `system.toml` section:** `[image.<id>.build] cmake_args =
   [..]`, CMake arguments only. Only the image's OWN section counts. A
   dependency's or third-party package's `system.toml` is never read for
   another image's arguments: it is **ignored** (a package is not this
   build's to refuse, and refusing would make a third-party package unusable),
   and `--dry-run` lists what it ignored. There is no `cargo_args` key: the
   one cargo argument a project would commit is `--features`, and committing
   features belongs in the package's manifest.
2. **The command line:** `--cmake-args="<one shell token>"` and
   `--cargo-args="<…>"`, with `=` required.
   - The value is split by plain POSIX quoting: no expansion, no `~`, and
     unbalanced quotes are refused.
   - The flags are repeatable, and their values add up in order.
   - The old space-separated form (`--cmake-args -DA -DB`) is refused, with a
     pointer to the `=` form.
   - `--profile=<name>` takes exactly what `[image.<id>] profile` takes today
     (a cargo profile or a CMake build type). It is a command-line spelling of
     that existing setting, not a new profile system (see "Deferred").
3. **An `@file`:** `--cmake-args=@build.args` reads one argument per line,
   with `#` comments. The path is resolved from the current directory. It is
   allowed on the command line only, and an `@file` cannot name another.

**One committed place per setting.** A setting the knob registry
(`packages/tooling/nros-zephyr-build/knobs.toml`, RFC-0103) owns already has
its committed home: the RFC-0049 app rung, `[image.<id>.knobs.<group>]`.
Any argument naming such a knob, in ANY of its spellings (its `NROS_<id>`
env name, its `CONFIG_*` symbol, a retired legacy name), is refused from
EVERY source, pointing at that table. So `[image.<id>.build] cmake_args`
holds only what no other key states (for example
`-DCMAKE_EXPORT_COMPILE_COMMANDS=ON`, `-Wno-dev`).

### D8 — Routing is built in, never a plugin

- CMake arguments go to every CMake configure: a cmake image's, west's cmake
  step, a cmake package's.
- Cargo arguments go to every cargo command.
- `--profile` maps to cargo's `--profile`, to `CMAKE_BUILD_TYPE`, and to
  west's passthrough, as `[image.<id>] profile` does today.
- A kind of argument that does not apply to an image's road is ignored, and
  `--dry-run` says so.
- `--` is kept only for west's own known options. Anything else after `--` is
  refused, naming the flag that carries it now.
- Arguments reach tools as argv only, never as a shell string.
- **The build dir records the arguments it was configured with** (today's
  one dir per image; per-profile dirs are deferred). On a change, the CMake
  variables that were removed are `-U`'d and the configure re-runs, and the
  build says so, so a `-DFOO` dropped from the command line does not linger
  in the cache.
- `--dry-run` prints the final argv for each tool and where each argument
  came from.

### D9 — One allowlist, the same for every source

Every source (D7) passes the same validation.

**Allowed.**
- CMake: `-D` in every spelling (parsed into name and value), `-U`, `-W*`,
  `--log-level`, `--log-context`, `--debug-find`.
- cargo: `--features`, `--no-default-features`, `--all-features`, `-v` /
  `-q`, `--timings`, `--locked` / `--offline` / `--frozen`. In a workspace with
  more than one package, a feature must be qualified (`--features=<pkg>/<feat>`).
  A bare feature name is refused there, because cargo would apply it to
  whichever packages define it.

**Refused, with a pointer to the setting that owns it.**
- CMake: `-C`, `-P`, `-G`, `-T`, `-A`, `--toolchain`, `--preset`, `-S`,
  `-B`, and anything else not listed above.
- cargo: `--config`, `-Z`, `--target`, `--target-dir`, `--manifest-path`,
  `--profile`, `--release`, and anything else not listed above.
- **`-DCONFIG_*`.** On the west road it sets a Kconfig symbol directly,
  bypassing the Kconfig fragments and the knob ladder (RFC-0098 D10/D11, the
  rule that a nano-ros knob is never authored in a conf file). Zephyr-native
  Kconfig goes in the image's `conf = [..]` fragment.
- **Owned `-D` names**: every `-D` name `nros build` itself passes. The table
  is GENERATED FROM THE CODE THAT EMITS THE ARGUMENTS, not written as a list.
  Each emitter (the configure-argument builders in
  `nros-cli-core/src/cmd/build.rs` and `builder/`) takes its names from one
  table it exports, and a test runs every road's argument builder and asserts
  each `-D` it produced is in the table. That covers the unprefixed ones a
  list would miss: `ARMFVP_EXTRA_FLAGS` (#4056; the `const` of that name in
  `cmd/build.rs`, which `fvp_extra_flags_configure_arg` formats from, so the
  table registers the const), the board heap `-D`,
  `CMAKE_TOOLCHAIN_FILE`, `CMAKE_PROJECT_*INCLUDE*`, and every `NROS_*` /
  `_NANO_ROS_*` name.
- **Knob-registry names**, in every spelling a row has: env, `CONFIG_*`,
  legacy (D7's "one committed place"). The refusal covers every spelling
  because it guards against the bypass. Its remedy names the row's
  user-facing spelling: the row's `surface` field in `knobs.toml`, which
  `nros knob show` also reads (coordinated with #4082). The Kconfig twin `CONFIG_NROS_*` is a
  carrier, never user surface: a nano-ros knob is never authored in a conf
  file (RFC-0098 D10/D11, gate `check-leaf-conf-nros-knobs`).
- **Derived features**: the RMW, platform and capability features. They are
  derived from the descriptors (the RMW from the backend's `nros-rmw.toml`
  and `[system] rmw`, the platform from the board descriptor's `platform`,
  capabilities from the board and system declarations), and they would split
  the build's choice between its Rust and C halves.

### Threat model

The arguments come from the user or from their own project, and a build
already runs that project's code. So the design guards against MISUSE:
silently re-wiring or corrupting a build.
- The allowlist also closes code execution through CMake (`-C`, `-P`) and
  cargo (`--config`, `-Z`).
- Secrets passed as arguments appear in logs. That is documented, not
  prevented.

### Tests (at least one for each risk)

- a CMake argument containing `--packages-skip`;
- every bypass spelling (each refused option, joined and separate forms);
- `-DCONFIG_<x>=y` from each source;
- an owned name the generated table catches (`-DARMFVP_EXTRA_FLAGS=…`);
- a knob in each spelling (`-DNROS_…`, `-DCONFIG_NROS_…`, a legacy name);
- a derived feature on the command line;
- a bare `--features=x` in a multi-package workspace, and `pkg/x` accepted;
- cache drift: `-DFOO`, then a build without it;
- `$(touch …)` in a value creates nothing;
- an unbalanced quote;
- `@file` resolution;
- a dependency's `system.toml` arguments ignored and listed by `--dry-run`;
- the `--dry-run` output.

### Migration

- **`--cmake-args` (colcon form).** The space-separated form is refused. The
  message is colcon-compatible in wording ("nros build takes
  `--cmake-args=<arg>`, one argument per flag, repeatable") and shows the
  user's own arguments rewritten.
- **`--` passthrough.** Only west's own options stay after `--`. A CMake or
  cargo argument there is refused with the flag that carries it now. A
  `--profile` / `--release` / `-DCMAKE_BUILD_TYPE` after `--`, which used to
  win over the image's profile, becomes `--profile=<name>`.
- **`[image.<id>] profile`** keeps its key and its meaning.

### Deferred: build profiles (was D10–D11 in the first draft)

User-defined profiles and per-profile build dirs move to a later amendment,
with their own phase. The review found three things they need first, recorded
here so the later amendment starts from them:

- **Name and file.** `[profile.<name>]` collides with cargo's own
  `[profile.*]` and would give two tables in two files pointing at each
  other. The later amendment renames it (working name `[build_profile.<name>]`)
  and names the one file it lives in.
- **One locator plan, priced.** `build/<image>/<profile>/` moves every
  artifact path: the fixture dirs, the sizing descriptor, the runner's
  `build_dir`, and every `model_location` consumer. Without one locator and
  its cost measured first, it repeats issue #393's class.
- **No custom `CMAKE_BUILD_TYPE`.** A custom type cannot be checked before
  configure, so only the four standard types are accepted.
- **Vocabulary (coordinated with #4082).** "Profile" means one thing to a
  user: the built-in choice `[image.<id>] profile` / `--profile=` takes, which
  `nros profile list|show` presents. The cargo plumbing (`nros dev profile
  args|dir|env`) is not a user noun. User-defined profiles get their own noun
  (`nros build-profile …`), never a second meaning of "profile".

## Alternatives considered

- **Delete the dev verbs from the CLI and make them library calls.** Their
  callers are cmake, west and shell, which cannot call Rust. A hidden tier
  keeps one binary as the answer to every question the build asks.
- **`nros plumbing`, after git.** The word describes the layer to someone
  who already knows git's split. `dev` describes the reader.
- **Rename everything to ROS names.** `--build-type` on `nros new` would
  read as a choice the user makes when it is derived (D4), and `colcon`'s
  `--merge-install` has no meaning without an install step (RFC-0087 D8).
- **Remove old spellings at the restructure.** Out-of-tree cmake files call
  the dev verbs, and a toolchain pin can move under them (RFC-0097). A
  removal would turn a pin bump into a broken configure with no warning.

## Open questions

1. Does `materialize` stay a user verb, or move under `nros new` as
   `nros new --materialize <entry>`? It is the user's last resort (RFC-0065
   D5), which argues for keeping it visible.
2. Should `nros list` grow `--kind runner|command|board|rmw`, replacing
   `nros dev ws providers` for users? The provider list is useful to a user
   adding a runner package (RFC-0119).

## Changelog

- 2026-11 — created, from the maintainer discussion of 2026-10-31 and
  2026-11-01.
- 2026-11 — amended by RFC-0120 (`nros configure`).
- 2026-11-03 — D4: `nros build` takes `-j/--jobs` (one shared jobserver,
  default the CPU count) and refuses colcon's `--parallel-workers`, naming
  `-j`. Maintainer decision: the package count is a workaround for colcon
  having no shared job pool.
- 2026-11-04 — D4: `--base-paths` has one meaning on every verb (the source
  roots, as RFC-0119 D4 defines them); `sync` lists it, and its earlier
  meaning there (replace the whole provider search path, underlay included)
  is gone. phase-527 W2d.
- 2026-11-07 — D2/D3: one user-tier `nros check <package>` dispatches on the
  package kind (runner, command, board); `nros dev runner|command check` stay
  as its aliases, and `board check` is that verb. User ruling, issue 2294.
- 2026-11-10 — amendment (user ruling): user-tier homes for the dev verbs
  issue 2394 lists: `nros configure --census [B:I]`, source metadata in
  `nros build` (under `--census` if it needs the native build), and
  `nros configure --migrate` for the bringup move; `nros doctor` stays
  read-only.
- 2026-11-10 — the amendment is implemented (issue 2394): `nros configure`
  takes `--census [BRINGUP:IMAGE]` and `--migrate`, `nros new` names
  `nros build` for source metadata, and `nros doctor` names `--migrate`
  without moving anything.
- 2026-11-14 — D2's table records the shipped tree (phase-555 W3, user ruling
  on review question N4/B25): `explain` and `update` are top-level, `plan`
  takes a bringup and no subcommands, `check` is top-level, `pin [VERSION]`,
  and `board list|show|info|new`.
- 2026-11-17 — amendment (DRAFT, user-approved design, for review): build
  arguments. `system.toml` / `--cmake-args=` / `--cargo-args=` / `@file`
  sources, built-in routing, one allowlist. Implemented by phase-568.
- 2026-11-17 — the amendment revised after review: profiles (D10–D11) deferred
  to a later amendment, with the review's three points recorded; `-DCONFIG_*`
  refused; the owned-names table generated from the emitting code; derived
  features' sources named; `pkg/feat` required in a multi-package workspace;
  knob-registry names refused from every source; system.toml takes
  `cmake_args` only; a dependency's arguments are ignored and listed.
- 2026-11-17 — the revised amendment (D7–D9) approved by the user. The RFC
  stays Draft until phase-568 lands it.
