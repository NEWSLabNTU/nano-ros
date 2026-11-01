# Phase 526 — The `nros` command tree in two tiers, with ROS 2 flag names and aliases

**Status (2026-11-01). IN PROGRESS — W1 and W2 done; W3 to W5 open.** Implements
[RFC-0117](../design/0117-cli-command-tiers.md). Comes before
[phase-525](phase-525-adoption-front-doors.md) W1, so the help-text id sweep
runs once, over the final verb names. Independent of phase-527 to phase-529,
except that the `run`, `flash`, `monitor` and `bench` slots it reserves are
filled there.

---

## Why

`nros --help` lists 33 verbs, of which seven exist only for cmake, west or
`just`, and `nros ws` holds 25 subcommands across five unrelated jobs.
RFC-0117 splits the tree: a task-grouped user tier and a hidden `nros dev`
tier, ROS 2 flag names wherever the meaning is the same, and hidden aliases
so nothing that calls an old spelling breaks.

## Work items

### W1 — The `nros dev` tier, with silent aliases

**Status: DONE** (the `nros dev` tier and `nros help --aliases` commits, landed together).

- Add `nros dev` and its nouns (`codegen`, `image`, `rmw`, `lock`,
  `profile`, `board`, `ws`, `sdk`, `metadata`) per RFC-0117 D3, each verb
  moved with its arguments, output and exit codes unchanged.
- Every old spelling stays as a hidden clap alias that runs the same code and
  prints nothing extra.
- `nros --help` stops listing the moved verbs; `nros help --all` and
  `nros dev --help` list them.

- `nros help --aliases [--json]` prints the alias table. The JSON form is
  `[{"old", "new", "tier"}]` with `tier = "dev"` for every W1 row; W2's user
  aliases join it with `tier = "user"`, and W4's gate reads it.

Acceptance:

- A test reads the alias table (`cmd::dev::ALIASES`), so it cannot miss a
  verb, and for every pair asserts that both spellings parse with the same
  clap shape and lower to the same `Cmd` (`Debug` equality). Runtime output
  and exit codes are identical by construction: one dispatch serves both
  spellings, since `Cmd::Dev` is lowered before anything runs. A second test
  asserts every `nros dev` verb has a row. Spot checks of `ws env`,
  `sdk-root`, `toolchain manifest`, `ws order` and `ws providers` measured
  byte-identical output.
- `nros --help` names no dev verb.

### W2 — The user tier regrouped, with warning aliases

**Status: DONE.** What landed, and the choices the bullets left open:

- `setup` absorbs `init` as `nros setup presets [dir]`. `toolchain list`,
  `toolchain gc` and `toolchain remove` replace `store list`, `store gc` and
  `toolchain uninstall`. `interface generate` replaces `generate`;
  `interface list` and `interface show` are new, with `ros2 interface`'s
  layout, `-m`/`-s`/`-a` filters and nested-type expansion, over the search
  path codegen resolves through plus the workspace's own `src/`. `list` and
  `clean` replace `ws list` and `ws clean`.
- `graph` prints the packages in `provider_scan`'s build order, each with the
  workspace packages it depends on. It is not `colcon graph`'s layout yet; W3
  owns the colcon flag and output names.
- `nros ws` is hidden from `nros --help`. `ws status` and `ws doctor` have no
  home in RFC-0117 D2, so they stay spelled under the hidden `ws` with no
  alias row; `ws router-config` and `ws qemu-nic` leave in phase-527 and
  RFC-0118.
- The D2 sub-verb forms the bullets do not name (`plan resolve|check|explain`,
  `pin update`, `board check`) are not moved here: `check`, `explain` and
  `update` stay top-level verbs, shown under Inspect and Toolchain.
  `generate-px4-msgs` is shown under Interfaces until phase-527 W5 moves it
  into a command package.
- `doctor --report` already printed the registry-backed report (phase-522
  W5), so nothing changed there.
- The user-tier rows join `nros help --aliases --json` with
  `tier = "user"`. The reserved verbs' names are `cmd::reserved::VERBS`, for
  phase-527 W4 to read.

- `setup` absorbs `init`; `toolchain list|remove|gc` absorbs `store`;
  `interface generate|list|show` gathers `generate` and the interface
  listing; `list` and `graph` replace `ws list` and `ws order` for users;
  `doctor --report` prints the registry-backed report RFC-0114 D12 names.
- Old user spellings stay as hidden aliases that print one line on stderr
  naming the new spelling, only when stderr is a terminal.
- Reserve `run`, `flash`, `monitor`, `bench` and `debug` in the tree. Until
  phase-528 and phase-529 fill them, each refuses and names what to do
  instead (the printed next step of `nros build`).

Acceptance:

- `nros --help` shows the RFC-0117 D2 groups in that order.
- A test asserts every old user spelling warns on a terminal and is silent
  when stderr is piped.

### W3 — ROS 2 and colcon flag names

- `build`: `--packages-select`, `--packages-up-to`, `--packages-skip`,
  `--packages-above`, `--continue-on-error`, `--cmake-args`,
  `--parallel-workers`, `--base-paths`, `--build-base`, with colcon's
  semantics over `provider_scan`'s topological order (RFC-0087 D7).
- `sync`: `--build-base`, with `--build-dir` as a hidden alias.
- `list`: `-t`, `-n`, `-p` as `colcon list`.
- `new`: the `ros2 pkg create` flags RFC-0117 D4 lists, keeping `--lang`,
  `--board`, `--template`.

Acceptance:

- For each flag taken from colcon, a test runs the same selection through
  `nros build --dry-run` and `colcon list` with the same flags over
  `packages/cli/testing_workspaces/ros2_rust_examples`, and asserts the same
  package set. Where colcon is absent, the test skips with
  `nros_tests::skip!`, never silently.

Decision step: if a colcon flag's semantics cannot be matched (for example
`--packages-above` across a cargo-only member), the flag keeps an `nros`
name and RFC-0117 D4's table records why.

### W4 — `check-cli-old-spellings` and the tree's own migration

- Write `scripts/check/check-cli-old-spellings.py`. It reads the alias table
  from `nros help --aliases --json`, which W1 adds as a dev flag, so the gate
  never carries its own list. It then refuses any old spelling in `cmake/`,
  `just/`, `scripts/`, `zephyr/`, `examples/`, `book/src/` and `docs/`,
  except RFC and phase docs quoting it in backticks beside the new spelling.
- Move every in-tree caller to the new spelling in the same commit as the
  gate.

Acceptance:

- The gate is on `just check fast` with a baseline of zero.
- The commit message carries the sweep command.
- A self-test plants one old spelling and sees the gate fail.

### W5 — Help text from `//` comments

- For every verb and flag, move the phase, issue or RFC history out of the
  `///` doc comment clap renders into a `//` comment above it. The `///` text
  says what the verb does.
- Hand over to phase-525 W1, whose `check-user-surface-ids` gate then
  starts at zero on the help surface.

Acceptance: the 238 id-bearing help lines measured on `c16e26f0b2` read zero
in `nros help --all` output; phase-525 W1's sweep covers only refusal strings
and the book.

## Acceptance

| Work item | Industrial | Field solutions | Education |
| --- | --- | --- | --- |
| W1 | build integrations keep working on a toolchain bump | — | — |
| W2 | — | `nros --help` fits a screen and groups by task | a lab handout names a short verb list |
| W3 | colcon users carry their selection habits over | — | ROS course material transfers |
| W4 | the tree never teaches a deprecated spelling | — | — |
| W5 | — | help says what a verb does | — |

## Out of scope

The run, flash, monitor and bench verbs (phase-528, phase-529), extension
commands (phase-527), and retiring any alias (a later RFC-0117 amendment
names the release).
