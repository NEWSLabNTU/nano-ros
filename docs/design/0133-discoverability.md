---
rfc: 0133
title: "Discoverability: every noun the build knows is listable, showable and completable"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-569]
supersedes: []
superseded-by: null
---

# RFC-0133 — Discoverability: every noun the build knows is listable, showable and completable

**Status:** Draft (2026-11-17). Requested by the user through the manager on
2026-11-17 ("a discoverability campaign … the way `ros2 pkg list` and
`ros2 interface list` work"), with dynamic shell completion added by the user.
Written for the user's review; nothing is built. The user answered the five
open questions on 2026-11-17 (recorded under "Decisions"); the RFC stays Draft
until the user approves it as a whole. The work breakdown is
[phase-569](../roadmap/phase-569-discoverability.md).

## Summary

A user should be able to find out what nano-ros offers — boards, RMWs,
platforms, configuration knobs, templates, runners, diagnostic codes — and
what nano-ros will do with their own workspace — its images, what the build
derived for each, and the nodes and entities each image will create — from
the `nros` command alone, without reading the source or the book first.

This RFC fixes four things: one grammar (`nros <noun> list` /
`nros <noun> show <name>`, `--json` on each), one source per noun (the data
the build itself reads, never a second table), discovery in the user tier
rather than under `nros dev`, and dynamic shell completion of names through
clap_complete's own engine, so <kbd>Tab</kbd> offers the same names `list`
prints.

## Motivation / problem

Today's `nros --help`, audited on 2026-11-17 (file references are to main at
that date):

- **Present:** `board list|show|info`, `interface list|show`, `list` and
  `graph` (packages), `config show|explain`, `toolchain list`,
  `setup --list`, `explain <code>`, `completions <shell>`.
- **Buried under the hidden `nros dev`:** `ws providers` (what every provider
  on the search path offers), `image sizing-descriptor` and
  `image entity-inventory` (what the build derived), `rmw dispatch --known`
  (which RMWs exist), `profile resolve`.
- **Missing:** an RMW list, a platform list, a catalog of configuration knobs,
  the derived configuration of an image and the reason for each value, the
  workspace's images, the nodes and entities an image will create, the `new`
  templates, runners and commands, a list of diagnostic codes, and a search
  across all of them.

Three further problems the audit found:

1. **Lists that disagree.** The RMWs are listed three times: the descriptor
   index the build uses (`RmwIndex`, from each `nros-rmw.toml`), the SDK
   index's `[rmw.*]` host package sets, and a hardcoded
   `value_parser = ["zenoh", "xrce", "cyclonedds"]` on `nros new --rmw`
   (`cmd/new.rs:114`). The descriptor index also knows `uorb`; the other two
   do not. Platforms have the same split: `PlatformsTree` (each
   `nros-platform.toml`) versus a static `OFFERED_PLATFORMS`
   (`cmd/new.rs:445`). Any list a user reads should be the one the build
   reads.
2. **Almost no `--json`.** Of the discovery verbs, only `board info` emits
   JSON (and nothing else). An editor plugin, a CI script or an agent has to
   scrape tables.
3. **Completion is static.** `nros completions <shell>` runs clap_complete's
   `generate`, which knows subcommands and flags but no names: no boards, no
   RMWs, no packages, and none of the extension verbs found at runtime.
   Nothing installs it, and `cmd/completions.rs`'s `run()` is a dead stub.

## Design

### D1 — One grammar

Every discoverable noun answers two verbs:

```text
nros <noun> list [filters] [--json]
nros <noun> show <name> [--json]
```

- `list` prints one row per item: the name, a one-line description, and the
  columns that tell items apart (for an RMW, the platforms it supports; for
  a board, its platform and status). Filters narrow the rows (`--platform`,
  `--rmw`, `--board`) and never change the columns.
- `show` prints everything about one item, ending with a **Book** line
  (D4). A name that matches nothing is refused with the nearest names
  (edit distance), the same way for every noun.
- `--json` exists on **every** `list` and `show`. It prints one object,
  `{"schema": "nros.<noun>.<verb>.v1", "items": [...]}` for `list` and
  `{"schema": …, "item": {...}}` for `show`. Keys are stable within a schema
  version; adding a key is not a version change, removing or renaming one
  is. A contract test (phase-569 W5) pins each schema.
- Human output goes to stdout, diagnostics to stderr, so
  `nros rmw list --json | jq` works when a warning is printed.

The nouns:

| noun | `list` answers | today |
| --- | --- | --- |
| `board` | which boards exist, their platform, status and setup set | `board list` lists board *crates*, not board keys |
| `rmw` | which RMWs exist, the platforms and boards each supports | buried: `dev rmw dispatch --known` |
| `platform` | which platforms (RTOSes and bare metal) exist, and their boards | missing |
| `interface` | message, service and action types on the search path | `interface list` (no `--json`) |
| `package` | the workspace's packages | `nros list` (kept, D3) |
| `knob` | every configuration item a user can state | missing; `config explain` shows one platform's values |
| `image` | the workspace's images and bringups: board, RMW, launch | missing |
| `node` | the nodes an image *will* create, from its plan | missing |
| `topic` | the topics, services and actions an image *will* use | missing |
| `template` | what `nros new` can scaffold: roles, languages, features | missing |
| `runner` | runners and extension commands on the search path | `dev runner check` only |
| `code` | the diagnostic codes `nros explain` knows | missing |
| `toolchain` | installed toolchains, and with `--available` the SDK index's | `toolchain list`, `setup --list` |

`nros node list` and `nros topic list` take `--image <name>` and read the
image's plan. They are labelled **planned** in human output and carry
`"source": "plan"` in JSON: they say what the build will create, not what is
running. A runtime view belongs to `nros monitor` and stays there.

### D2 — One source per noun

Each `list`, `show`, completion and search reads the data the build reads,
through the loader the build calls. No noun gets a table of its own.

| noun | source | loader | cost |
| --- | --- | --- | --- |
| `board` | `nros-board.toml` descriptors; `board-support-status.json` for status | `BoardCatalog::load`; `board_support::Status::load` | file reads |
| `rmw` | `nros-rmw.toml` descriptors | `rmw_index::for_workspace` / `for_sdk_root` | provider scan, memoised; cached in `build/nros/providers.json` |
| `platform` | `nros-platform.toml` | `PlatformsTree::load` | file reads |
| `interface` | the interface search path | `AmentIndex` (as today) | directory walk |
| `package` | workspace scan | `scan_for_query` (as today) | directory walk |
| `knob` | `knobs.toml`, compiled into `KNOBS` | `knob_by_toml`, extended (below) | none: compiled in |
| `image` | the workspace's bringups (`system.toml`) | the configure scan | file reads |
| `node`, `topic` | `build/nros/nros-plan.json` | the plan reader `nros explain plan` uses | file read; needs a prior `nros configure` |
| derived config | `build/nros/sizing/<entry>.toml` | `nros_sizing_descriptor::read` | file read; needs a prior `nros configure` |
| `template` | `scaffold::Role::ALL` | `template_values()` | none: compiled in |
| `runner` | package.xml announcements | `extension::discover` | provider scan |
| `code` | `nros_diagnostic_codes::ALL` | `lookup` | none: compiled in |
| `toolchain` | the store; `nros-sdk-index.toml` | `store::scan`; `SdkIndex::load` | file reads |

Consequences, each a work item:

- **The second and third RMW lists go.** `nros new --rmw` and
  `nros new --platform` take their candidates and their validation from
  `RmwIndex` and `PlatformsTree`. The SDK index's `[rmw.*]` stays (it says
  which host packages each RMW needs) but a gate refuses an `[rmw.*]` with no
  descriptor and a descriptor with no `[rmw.*]`.
- **The knob registry becomes the catalog.** Each `[[knob]]` row in
  `knobs.toml` gains what a user needs to state it: `scope` (image, platform,
  board or workspace), `default`, `values` (a range or an enumeration),
  `set-in` (which of `system.toml`, Kconfig and the environment accept it,
  with the spelling in each; the Kconfig and env twins already exist), and
  the `prompt` and `help` text, which today live only in the TOML and are not
  compiled in. The public / carrier / dead classification
  `gen-config-surface.py` computes in Python moves into the row, so the CLI,
  the book page and the gate read one field. `nros config explain` becomes
  `nros knob list --platform <p>` (the resolved values and the rung each came
  from) and stays as an alias.
- **Derived values say why.** `nros image show <image> --derived` prints
  what the build derived for the image — pool sizes, heap, stack, enabled
  features — and, for each, the reason: the knob or the plan fact it came
  from. The sizing descriptor records values today, not reasons, so the
  descriptor gains a `because` field per derived value, written by the code
  that derives it (phase-569 W3). Until a value has one, it prints
  "(no reason recorded)", which a test counts down. The first values to
  carry a reason are the ones that went wrong in the week of 2026-11-16: the
  pthread condition-variable and mutex pools, the executor backing, the
  parameter store slots, the Zephyr heap, and the stack setting against the
  floor the stack check measured. The entity pools follow.
- **Plan-dependent nouns never build.** `node`, `topic` and `--derived` read
  what `nros configure` wrote. If it is missing or older than the workspace's
  inputs, they say so and name `nros configure`; they do not run it.

### D3 — Discovery lives in the user tier

Discovery verbs are user verbs, shown in `nros --help` under a
"Find what nano-ros offers" group (RFC-0117 groups). `nros dev` keeps the
build-system plumbing: facts in CMake or `KEY=VALUE` form, model paths,
session headers, lock maintenance.

Migration (the old spelling stays as a hidden alias, recorded in the
old-spellings table so `check-cli-old-spellings` and the book see one name):

| today | becomes | note |
| --- | --- | --- |
| `nros dev rmw dispatch --known` | `nros rmw list` | `dispatch <name>` (build facts) stays in `dev` |
| `nros dev ws providers [--kind K]` | `nros <K> list` for `rmw`, `board`, `platform`, `runner`; `nros dev ws providers` stays for kinds with no noun | |
| `nros dev image sizing-descriptor` | `nros image show <image> --derived` | the TOML stays available as `--json` |
| `nros dev image entity-inventory` / `entity-census` | `nros node list --image` / `nros topic list --image` | |
| `nros dev profile resolve` | `nros profile list` / `show` | `profile args`, `dir`, `env` stay in `dev` |
| `nros config explain --platform P` | `nros knob list --platform P` | alias kept |
| `nros setup --list` | `nros toolchain list --available` | alias kept |
| `nros list` | `nros package list` | `nros list` stays the short spelling, as `ros2 pkg list` has no shorter one |
| `nros explain <code>` | `nros code show <code>` | `nros explain <code>` stays as an alias, because diagnostics already tell users to run it; `nros explain plan` is unchanged |
| `nros board list` | unchanged spelling, now one row per board **key** | the crate becomes a column |

Nothing is removed in this RFC. Removing an alias is a later decision, with
its own release note.

### D4 — Every `show` links its book chapter

A `show` ends with `Book: <url>`. The page comes from the noun's own data,
not from a table in the CLI:

- boards: `book_run` / `book_flash` in `board-support.toml` (today printed as
  repo paths);
- RMWs: a new `book` field in `nros-rmw.toml`;
- platforms: a new `book` field in `nros-platform.toml`;
- knobs: an anchor on the configuration reference page, generated from the
  same registry row;
- codes: an anchor on the diagnostic codes page, generated from the same
  table;
- templates, runners: the page that introduces them, in their source data.

The links point at the book CI already deploys: `publish.yml` copies the
built book to `NEWSLabNTU.github.io`, under `nano-ros-book/`, so the base URL
is `https://newslabntu.github.io/nano-ros-book/`. It is defined **once**, in
the CLI workspace's manifest, and compiled in through a build script, so
moving the site changes one line. `book/book.toml`'s `site-url` is checked
against the same value. A gate checks that every `book` value names a page in
`book/page-map.toml`.

That site is built from main, not per release, so an older installed `nros`
links the current book. A page that moves keeps a redirect
(`[output.html.redirect]`), so old links still land.

### D5 — No ids in user-facing output

Every `list` and `show`, in both forms, is user-reachable text and passes
`check-user-surface-ids`: no issue, phase, RFC or work-item numbers. The
checker already renders help and `board list`/`show`; it gains each new verb.

### D6 — `nros search <word>`

`nros search <word>` matches the word, case-insensitively, against the name,
aliases and description of every item in every noun whose source is cheap
(everything in D2 except the plan-dependent ones, which join when a plan
exists). Results are grouped by noun, and each row names the `show` command
that tells more. Illustrative output (the names are real registry rows; the
summaries are placeholders until W2 compiles the help text in):

```text
$ nros search socket
knob   net.max_sockets         Sockets the network layer can hold open   nros knob show net.max_sockets
knob   net.max_udp_sockets     UDP sockets of those                      nros knob show net.max_udp_sockets
knob   net.socket_timeout_ms   How long a socket call waits              nros knob show net.socket_timeout_ms
```

`--json` gives `{"schema": "nros.search.v1", "items": [{"noun", "name",
"summary", "show"}]}`. Search has no index of its own: it calls each noun's
`list` loader.

### D7 — Dynamic shell completion

**Engine.** clap_complete's own dynamic completion (`unstable-dynamic`
feature): `CompleteEnv::with_factory(Cli::command).complete()` at the top of
`main`, after the toolchain-pin redispatch (so the pinned `nros` answers) and
before anything writes to stdout. Name arguments carry
`ArgValueCandidates` whose functions call the D2 loaders; extension verbs
come from `SubcommandCandidates` over `extension::discover`. No shell script
is written by hand: clap_complete generates each shell's glue.

Completed names: board keys and aliases, RMWs, platforms, packages, images,
knob names, interface types, template roles, codes, toolchains, and
extension verbs.

**Fast, and never builds.** A completion runs on every <kbd>Tab</kbd>:

- the budget is **100 ms per <kbd>Tab</kbd>**, including the redispatch
  exec of the pinned binary;
- candidate functions read only what D2 marks as a file read or compiled in.
  Names that could take longer — packages and images in a big workspace,
  interface types, workspace providers — come from a small completion cache,
  `build/nros/complete.json`, that `nros build` and `nros configure` rewrite
  as a side effect of the scans they already do. When the cache is missing,
  those names are not offered; the compiled-in and SDK-root names still are;
- a candidate function never runs a scan that writes, never runs cargo,
  CMake, `nros build` or `nros configure`, never touches the network, and
  never writes a file;
- a test runs each completer against the repo workspace on a cold page cache
  and fails above the budget;
- an error in a candidate function yields no candidates, never a message on
  the terminal.

**Registration.** clap_complete recommends sourcing the glue at shell start,
so a new `nros` brings matching glue:

| shell | line |
| --- | --- |
| bash | `source <(COMPLETE=bash nros)` in `~/.bashrc` |
| zsh | `source <(COMPLETE=zsh nros)` in `~/.zshrc`, after `compinit` |
| fish | `COMPLETE=fish nros \| source` in `~/.config/fish/completions/nros.fish` |
| elvish | `eval (E:COMPLETE=elvish nros \| slurp)` in `~/.elvish/rc.elv` |
| PowerShell | `$env:COMPLETE = "powershell"; nros \| Out-String \| Invoke-Expression; Remove-Item Env:\COMPLETE` in `$PROFILE` |

- **`nros completions <shell>`** prints that shell's line, and
  `nros completions` with no shell picks it from `$SHELL`. With `--static`
  it keeps today's behaviour (the `generate` script, no names), for
  packagers who install a completion file. The dead `run()` stub is deleted.
- **Installed `nros`:** `scripts/install.sh` prints the completion line for
  the user's shell beside the PATH line it already prints. It does not edit
  rc files.
- **`nros setup`** prints one line after its **first** successful run: the
  command for the user's shell (from `$SHELL`), then records that it did, in
  `$NROS_HOME`, so the tip never repeats. It does not try to detect whether
  completion is already active.
- **Contributors:** `activate.sh` sources the glue when the shell is
  interactive bash or zsh and the in-tree `nros` exists; `activate.fish`
  does the same for fish.
- **Book:** Install gains a short "Turn on completion" step after the PATH
  step, with the table above, and the CLI reference says completion offers
  names.

**Stability.** `unstable-dynamic` is not covered by clap_complete's semver.
The workspace pins `clap_complete` to an exact version, and a test runs each
shell's glue against `nros` (bash and zsh and fish in the gate image; elvish
and PowerShell when present) so an upgrade that changes the protocol fails
CI, not a user's shell.

## Alternatives considered

- **`nros list <noun>`** (one verb, noun as argument). Shorter to type, but
  ROS 2 users know `ros2 <noun> list`, and `show` needs a noun-specific
  argument that completion and help can only describe per noun.
- **A discovery table kept in the CLI.** Cheapest to write, and exactly how
  the three RMW lists came to disagree. Refused by D2.
- **Static completion with names baked in at build time.** No startup cost,
  but stale the moment a workspace adds a package or an extension, and it
  cannot see images. clap_complete's dynamic engine is the toolchain-native
  way, as the user prefers.
- **Hand-written completion scripts per shell.** Full control, five scripts
  to keep in step with the command tree. Only worth it if `unstable-dynamic`
  proves unusable; the stability test is what would tell.
- **Writing rc files from `install.sh` or `nros setup`.** Saves a step, but
  editing a user's shell start-up without asking is the kind of change a
  user should make. Printed instead.

## Decisions

The user's answers of 2026-11-17, relayed by the manager:

1. **Book URL.** The github.io site CI already deploys
   (`newslabntu.github.io/nano-ros-book/`), with the base URL defined once in
   the build so it can move (D4).
2. **Completion tip.** One line after the first successful `nros setup`, with
   the command for the user's shell, then recorded so it never repeats (D7).
3. **Codes.** `nros code list|show` in the shared grammar; `nros explain
   <code>` stays as an alias, because diagnostics already tell users to run
   it (D3).
4. **Completion budget.** 100 ms per Tab. Names that would take longer come
   from a small cache that `nros build` and `nros configure` refresh, never
   from a build (D7).
5. **Reasons first** (the manager's call, which the user may revise): the
   pthread condition-variable and mutex pools, the executor backing, the
   parameter store slots, the Zephyr heap, and the stack setting against the
   measured floor; then the entity pools (D2).

## Open questions

None. The RFC waits for the user's approval as a whole.

## Changelog

- 2026-11 — created (Draft), for the user's review.
- 2026-11 — the user's five answers folded in (Decisions); still Draft.
