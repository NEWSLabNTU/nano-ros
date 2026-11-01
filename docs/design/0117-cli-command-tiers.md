---
rfc: 0117
title: "CLI command tiers: a task-grouped user tier, a hidden `nros dev` tier, ROS 2 flag names where the meaning is the same, and aliases instead of removals"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-526]
amends: [0116]
supersedes: []
superseded-by: null
---

# RFC-0117 — CLI command tiers

**Status:** Draft (2026-11-01). Written from the maintainer discussion of
2026-10-31 and 2026-11-01 on how the `nros` command tree should look to a
user and to a contributor. Its two siblings are
[RFC-0118](0118-target-execution-model.md) (how an image is run, flashed and
monitored) and [RFC-0119](0119-cli-extension-packages.md) (packages that add
runners and commands). Implemented by
[phase-526](../roadmap/phase-526-cli-two-tier-restructure.md).

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
  packages, `nros new` scaffolds one, and `nros sync` names a build directory,
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

| Group | Verbs |
| --- | --- |
| Start | `new` · `setup` (absorbs `init`) |
| Build | `build` · `sync` · `clean` · `materialize` |
| Run on a target | `run` · `flash` · `monitor` · `bench` · `debug` (reserved) — RFC-0118 |
| Workspace | `list` · `graph` |
| Interfaces | `interface generate\|list\|show` |
| Inspect | `doctor` · `board list\|info\|new\|check` · `config show\|explain` · `plan resolve\|check\|explain` |
| Toolchain | `toolchain list\|remove\|gc` (absorbs `store`) · `pin show\|bump\|update` · `locate` |
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
| `dev runner` / `dev command` | new: `check` for an extension package — RFC-0119 |

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
| `build` | `--packages-select`, `--packages-up-to`, `--packages-skip`, `--packages-above`, `--continue-on-error`, `--cmake-args`, `--parallel-workers`, `--base-paths`, `--build-base` | — |
| `sync` | `--build-base` (was `--build-dir`) | — |
| `list` | `-t` (topological), `-n` (names only), `-p` (paths only), as `colcon list` | — |
| `graph` | as `colcon graph` | — |
| `new` | `--dependencies`, `--node-name`, `--library-name`, `--license`, `--maintainer-name`, `--maintainer-email`, `--description`, `--destination-directory` | `--lang` (not `--build-type`: our package's build type is derived from the language and the board, RFC-0087 D2, so the ROS flag would name a choice the user does not make); `--board`, `--template`, `--kind` (no ROS counterpart) |
| `interface` | `list`, `show` as `ros2 interface` | `generate` (no ROS counterpart) |
| `doctor` | `--report` as `ros2 doctor` | — |
| `run` | `--prefix` as `ros2 run`; arguments after `--` | `--expect`, `--timeout`, `--router` (no ROS counterpart) |

A flag with no ROS counterpart follows the existing `nros` spelling.

### D5 — Aliases, never removals

Every old spelling resolves, hidden from help:

- An **old user-tier spelling** (`nros init`, `nros store gc`,
  `nros ws list`) works and prints one line on stderr naming the new
  spelling. The line is suppressed when stderr is not a terminal, so a script
  that captures output sees no change.
- An **old dev spelling** (`nros image-facts`, `nros ws board-facts`) works
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
