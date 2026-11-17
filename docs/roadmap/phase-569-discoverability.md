# Phase 569 — find what nano-ros offers from the `nros` command

**Status (2026-11-17). OPEN: W1 is done; W2–W10 are open.
The user approved the design on 2026-11-17; recording a reason per derived
value is [phase-570](phase-570-derived-value-reasons.md).**
Implements [RFC-0133](../design/0133-discoverability.md). The request came
from the user through the manager on 2026-11-17: a discoverability campaign
in the style of `ros2 pkg list` and `ros2 interface list`, plus dynamic
shell completion. Owner: the docs/UX session; it starts with W1.

## Why

The RFC's Motivation has the audit. In short, a user cannot list the RMWs,
platforms, configuration knobs, templates, runners or diagnostic codes, or
see what the build derived for an image, without reading the source. Three
lists of RMWs disagree, only one discovery verb emits JSON, and completion
offers no names.

## Order

The order puts the cheapest, highest-value items first. Each item's data
source already exists (RFC-0133 D2), except W3's `because` field.

| item | what | depends on |
| --- | --- | --- |
| W1 | `nros rmw list` / `show` | — |
| W2 | the knob catalog: `nros knob list` / `show` | — |
| W3 | derived config: `nros image show --derived` | reasons fill in as phase-570 lands |
| W4 | dynamic completion and its registration | W1, W2 (their loaders) |
| W5 | the shared `list` / `show` / `--json` / Book frame; existing verbs adopt it | W1 |
| W6 | `platform`, `template`, `profile`, `code`, `runner` lists | W5 |
| W7 | `image list` / `show`, with planned `--nodes` / `--topics` | W5 |
| W8 | migration out of `nros dev`, aliases, old-spelling table | W1–W7 |
| W9 | `nros search` | W5–W7 |
| W10 | the book: a "Find what nano-ros offers" page; CLI reference | each item adds its rows |

## Work items

### W1 — `nros rmw list` / `show`

- **Status:** DONE (2026-11-17)

Landed as specified, with three findings:
- no source stated which boards an RMW runs on except prose in the board
  support status; the generator now writes it as data (`rmws`: runtime and
  build-only, canonical names), from the same matrix cells as the prose;
- an RMW's summary is the first paragraph of its `package.xml` description
  (the rest is contributor notes);
- `nros-sdk-index.toml` had no `[rmw.uorb]`; it now states `packages = []`,
  and `check-rmw-descriptors` S5 holds the index and the backends to one
  list. `nros new --rmw` accepts any indexed name, so `uorb` is accepted
  there too; a scaffold whose board has no test over it is not refused yet.

`nros rmw list` prints one row per RMW from `RmwIndex` (each
`nros-rmw.toml`): name, summary, and the platforms and boards it supports.
`nros rmw show <name>` adds the host packages it needs (from the SDK index's
`[rmw.*]`), the Kconfig and `system.toml` spellings that select it, and its
Book line. `nros new --rmw` takes its candidates and validation from the same
index; its hardcoded list goes. A gate refuses an `[rmw.*]` entry with no
descriptor, and a descriptor with no entry.

Acceptance: `nros rmw list` names zenoh, xrce, cyclonedds and uorb on main;
`--json` matches a pinned schema; `nros new --rmw uorb` is accepted or refused
by the descriptor's own platform list, not by a list in `new.rs`.

### W2 — the knob catalog

- **Status:** OPEN (W2a and W2b on one branch; W2c open)
- **W2a, the catalog fields:** done. Each row authors `scope` and `class`
  (public, carrier, internal, retired; a carrier names what it `carries`).
  The generator derives `default` (the Kconfig `default` lines with their
  conditions, else the ladder builtin, else unstated), `values` and
  `surface`, and refuses a row that authors any of the three, or whose last
  Kconfig default is conditional. Two shrink-only lists name what the rows
  cannot yet say: 13 public rows with no default outside the code that
  reads them (`NO_DEFAULT`) and 32 with no help (`NO_HELP`).
- **W2b, the verbs:** `nros knob list [--json] [--platform P
  [--board-toml F]]` and `nros knob show <knob> [--json]`
  (`nros.knob.list.v1`, `nros.knob.show.v1`). A ladder default is resolved
  through the same function `config explain` prints (`resolve_ladder`).
  The registry's help was written for menuconfig, so 54 public help texts
  name `NROS_*`, `CONFIG_*` or `ZPICO_*` spellings; `show` replaces every
  knob's spelling with its name. A conditional default is shown with each
  case, in words (`CONDITIONS`; a test refuses a condition with none).
- **W2c, open:** the configuration reference generated from the rows, with
  the gate that compares it to `nros knob list`; `gen-config-surface.py`
  reading `class` from the rows (its prompt test now calls every knob
  symbol "derived", because phase-484 W5b made them prompt-less);
  `config explain` as an alias; the `surface` field handed to the
  build-arguments refusal (nros 118).
- **Findings:** `memory.heap_bytes` and `memory.app_stack_bytes` are ladder
  knobs with no registry row; the FreeRTOS and ThreadX `*_KB` rows carry
  them.

Each `[[knob]]` row in `knobs.toml` gains `scope`, `default`, `values`,
`surface` (the one user spelling, `[image.<id>.knobs.<group>]` or the rung
the row names) and the public / carrier class that `gen-config-surface.py`
computes today. `prompt` and `help` are compiled into `KnobRow`.
`nros knob list` prints every public knob (name, scope, default, one line of
help); `--platform P` / `--board B` add the resolved value and the rung it
came from, which is what `config explain` prints today. `nros knob show
<name>` prints the user spelling from `surface`, the valid values and the
help. It never prints the env carrier, the Kconfig twin or a `-D NROS_*`
spelling. The build-arguments refusal (RFC-0117, as amended) reads the same
field for its remedy. `config explain` becomes an alias. The configuration reference
page is generated from the same rows.

Acceptance: every one of the 110 rows has the new fields, or the generator
refuses; `nros knob show net.max_sockets` prints its `system.toml` spelling
and no env or Kconfig name; the
book page and `nros knob list` list the same public knobs (a gate compares
them).

### W3 — derived config

- **Status:** OPEN

`nros image show <image> --derived` prints, from
`build/nros/sizing/<entry>.toml`, the pools, heap, stack and features the
build derived, with a reason column. The column shows the reason a producer
recorded, which phase-570 adds, and "(no reason recorded)" until then. With
no descriptor, or one older than the image's inputs, the command names
`nros configure` and runs nothing.

Acceptance: a configured image shows its derived values, with the reason
column; an unconfigured one names `nros configure` and exits non-zero
without building.

### W4 — dynamic completion and its registration

- **Status:** OPEN

Enable clap_complete's `unstable-dynamic`, pinned to an exact version.
`CompleteEnv::with_factory(Cli::command).complete()` goes in `main` after the
toolchain-pin redispatch. `ArgValueCandidates` on every name argument
(boards, RMWs, platforms, packages, images, knobs, interface types,
templates, codes, toolchains) and `SubcommandCandidates` for extension
verbs, each calling only compiled-in tables, file reads and the completion
cache `build/nros/complete.json`, which `nros build` and `nros configure`
rewrite from the scans they already run (RFC-0133 D7). The cache records a
schema version and its search path; a cache for another search path offers
nothing, and stale entries within one search path are expected. The
redispatch costs every pin form under 5 ms (measured; the stale-binary guard
never runs for `COMPLETE=`), so nothing in it needs skipping.

`nros completions [<shell>]` prints the registration line for the shell, and
`--static` keeps today's generated script. The dead `run()` stub goes.
`install.sh` prints the line for the user's shell. `nros setup` prints it
once, after its first successful run, and records that in `$NROS_HOME` so
it never repeats.
`activate.sh` and `activate.fish` source the glue for interactive shells.
Install gains a "Turn on completion" step.

Acceptance:
- bash, zsh and fish each complete `nros board show <Tab>` with board keys,
  in a test that runs the real glue;
- every completer stays under 100 ms per Tab on a cold page cache, the
  redispatch exec included, for a version pin and for a `path =` pin;
- a cache written for another search path offers no names;
- with no completion cache, workspace names are simply not offered, and no
  completer runs a scan;
- a second `nros setup` prints no tip;
- a completer run under `strace -f` executes no cargo, cmake or network
  call, and writes no file.

### W5 — the shared frame, and existing verbs adopt it

- **Status:** OPEN

One helper renders `list` (table or `--json`), `show` (fields, then the Book
line) and the unknown-name refusal with the nearest names. `board`,
`interface`, `toolchain` and `nros list` adopt it and gain `--json`.
`board list` lists board keys, with the crate as a column. `board show`
renders every table in the board's descriptor, labelled from the schema's
field documentation, so sections such as `[board.can]` and
`[board.run.fvp]` appear with no CLI edit. A contract test
pins each `--json` schema, and `check-user-surface-ids` renders every new
verb. The Book base URL, `https://newslabntu.github.io/nano-ros-book/`, is
defined once in the CLI workspace manifest and compiled in, and a gate checks
`book/book.toml`'s `site-url` against it.

Acceptance: every discovery verb accepts `--json`; the contract test fails on
a renamed key; `check-user-surface-ids` covers the new verbs; a fixture
descriptor with a new section is printed by `board show`.

### W6 — `platform`, `template`, `code`, `runner` lists

- **Status:** OPEN

- `nros platform list` / `show` read `PlatformsTree`, with its boards and
  RMWs; `nros new --platform` reads the same list.
- `nros template list` / `show` read `scaffold::Role::ALL`: roles, languages
  and features.
- `nros profile list` / `show` read the built-in profiles that
  `[image.<id>] profile` and `--profile=` accept, each with the cargo profile
  and CMake build type it maps to, through the build's own resolver.
  `nros dev profile args|dir|env` stays plumbing. A later
  `[build_profile.<name>]` gets its own noun.
- `nros code list` / `show` read the diagnostic code table. `nros explain
  <code>` stays as an alias, because diagnostics tell users to run it, and
  `nros explain plan` is unchanged.
- `nros runner list` / `show` list runners and extension commands from
  `extension::discover`.

Each `show` has a Book line from its own data (new `book` fields where
needed).

Acceptance: each list matches its source in a test that adds a fixture item
and sees it listed, with no CLI edit.

### W7 — images, and what an image will create

- **Status:** OPEN

`nros image list` prints the workspace's images and bringups (board, RMW,
launch file). `nros image show <image>` adds its packages and, with
`--derived`, W3's view. `--nodes` and `--topics` print the nodes, and the
topics, services and actions, from the plan, in a column headed planned.
Without a plan they name `nros configure`. No `nros node` or `nros topic`
noun exists: those names stay with the runtime.

Acceptance: the Tutorial's workspace lists its images; its talker image's
planned nodes and topics match what `ros2 node list` and `ros2 topic list`
show when it runs (a probe step).

Also here: [issue 2667](../issues/2667-no-user-verb-for-no-alloc-check.md),
a user verb on the image noun that runs the allocation check
(`check-no-alloc-image`) on a built image, which today only a checkout can run.

### W8 — migration out of `nros dev`

- **Status:** OPEN

Apply RFC-0133 D3's table. Each moved spelling stays as a hidden alias and
gets a row in the old-spellings table. `nros --help` gains the
"Find what nano-ros offers" group.

Acceptance: every row of the D3 table works under both spellings;
`check-cli-old-spellings` reports each old one; build scripts that call
`nros dev …` keep working unchanged.

### W9 — `nros search <word>`

- **Status:** OPEN

Search the name, aliases and description of every item in every cheap noun,
plus each image's planned nodes and topics when a plan exists, by calling each noun's `list` loader.
Results are grouped by noun, with the `show` command to run; `--json` gives
`nros.search.v1`.

Acceptance: `nros search zenoh` returns the RMW, its knobs and the boards
that support it; a fixture item added to any source is found with no search
code change.

### W10 — the book

- **Status:** OPEN

A user-guide page, "Find what nano-ros offers", walks through `list`, `show`,
`search` and completion with real output. The CLI reference is regenerated.
Each item above adds its own rows to the page as it lands.

Acceptance: the page passes the book gates; each command on it is a probe
step on the installed track.

## Open questions

None: the user answered all five on 2026-11-17 (RFC-0133 "Decisions").
