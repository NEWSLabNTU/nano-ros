# Phase 569 — find what nano-ros offers from the `nros` command

**Status (2026-11-26). OPEN: W1, W3 and W7 are done; W2, W4, W5 and W6 are
largely merged, each with one part open; W8–W10 are open.
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

- **Status:** OPEN. W2a and W2b merged in #4267 (5f8c91ae5e, 2026-11-19);
  W2c is open: no generated configuration reference or comparing gate on
  main (audited 2026-11-26).
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

- **Status:** DONE (2026-11-24; rebased on main after W7c part 1 merged)
- **Measured 2026-11-24** on a fresh copy of `examples/linux/rust/talker`:
  - after `nros configure`, `nros image show linux --derived` prints the
    DERIVED / VALUE / REASON table and the descriptor it read;
  - an unconfigured copy names `nros configure`, exits 1, and writes no
    `build/`.
- **Fixed on the way:**
  - a refusal that lists types filled one table row with the whole list.
    The row keeps the lead clause and points to `--json`.
  - an error from `nros` ended with a `Location:` block naming the
    nros source line. It no longer does, unless `RUST_BACKTRACE` is set.
- **Done:**
  - Each plan records where its image's sizing descriptor lands
    (`ResolvedBuild::sizing_descriptor`), from one function
    (`sizing_descriptor_at`) that takes each road's writer directory:
    - cargo: the generated entry dir, with the image id;
    - Zephyr Rust: the resolve dir, with the image id;
    - Zephyr C/C++ and cmake: the CMake binary dir, with the entry target;
    - the leaf roads: their own rule.
    A query plans no resolve dir or output, so for those it calls the rules
    that spell them.
  - `--derived` prints a DERIVED / VALUE / REASON table. A stated value reads
    "(no reason recorded)", and one the producer refused reads "not derived:
    <its refusal>".
  - The list of values is `sizing_descriptor::derived_facts`, which
    `nros dev image sizing-descriptor`'s summary now prints from too.
  - A missing descriptor names `nros configure`. Freshness is the plan's:
    the model gate refuses a model older than its inputs. The descriptor's own
    mtime is not used, because it is written only when it changes.
- **Not in the descriptor yet:** stack and enabled features. The table shows
  what the descriptor states (target, entity and type counts).

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

- **Status:** DONE (2026-11-28). W4a merged in #4473 (ae294738f5), W4b
  in #4510 (9b2cece484) and W4c in #4561 (f3377cc188), all 2026-11-20/21;
  W4d lands with the interface-type completer below.
- **W4a, the engine and its registration:** done.
  - `clap_complete` is pinned to `=4.6.3` with `unstable-dynamic`, in
    `nros-cli` and `nros-cli-core`.
  - `CompleteEnv::with_factory(Cli::command).complete()` runs in `main`
    right after the toolchain-pin redispatch.
  - `cmd::complete` holds the completers: board keys and aliases (the board
    catalog `nros board list` reads), RMW names and aliases (the RMW index),
    and public knob names with their summaries (the compiled-in registry).
    They are on every board-name argument of `nros board`, on
    `nros rmw show` and on `nros knob show`. A source a completer cannot
    read offers no names.
  - `nros completions [<shell>]` prints the registration line for the shell
    `$SHELL` names (or the one named), with the startup file it goes in.
    `--static` prints the old generated script. The not-implemented stub is
    gone.
  - `activate.sh` registers the glue in interactive bash and zsh (zsh after
    `compinit`), `activate.fish` in interactive fish. `install.sh` prints
    the line. The first `nros setup` that provisions prints it once, and
    records that in `$NROS_HOME/state/completion-tip-shown`.
  - The book's Install page gains step 5, "Turn on completion".
  - **Tests:**
    - `nros-cli/tests/completion_glue.rs` sources the real bash glue and
      completes `nros board show <Tab>` (board keys) and `nros rmw show
      zen<Tab>` (`zenoh`).
    - The fish and zsh glue were driven by hand on newslab-server139: fish
      with `complete -C`, and zsh with `_describe` stubbed, since zsh's
      completion functions run only inside its line editor. Both returned
      the board keys and `zenoh`. They have no test yet: no gate lane claims
      zsh or fish, so a test needing them needs a capability first (see W4b).
    - One completion took about 7 ms with a checkout `nros`.
- **W4b, the completion cache, workspace names and acceptance:** done
  locally (2026-11-21); it opens as a PR after W4a merges.
  - `nros configure` and `nros build` write `build/nros/complete.json` at
    the end of `configure_project`. It holds the schema version, the
    provider search path (`ws::rmw_search_path_for`, the RMW index's key),
    the images as a user types them (bare where one bringup declares the id),
    the packages, the workspace's own RMW providers, and its top-level
    extension verbs. Writing it never fails a configure.
  - Completers read it only after checking the schema and the search path:
    a cache built for another search path offers nothing. With no cache,
    workspace names are not offered.
  - The RMW completer no longer builds the workspace's index (a scan). It
    offers the SDK root's backends plus the cached providers.
  - Images complete on `nros build`, `run`, `flash`, `image-facts` and
    `configure --image`; packages on the four `--packages-*` selections;
    boards on `nros setup <board>`, `new --board`, `build --board` and
    `doctor --board`; RMWs on `setup --rmw`, `new --rmw` and `plan --rmw`.
    Extension verbs complete at `nros <Tab>` (`SubcommandCandidates`).
  - **Acceptance:**
    - `every_completer_answers_within_the_budget` runs seven completions
      through the binary and holds each under 100 ms. It runs warm: dropping
      the page cache needs root, so "cold" was not measured.
    - `strace -f` of five completers on newslab-server139 showed one execve
      (the binary itself), no connect and no file opened for writing.
    - Unit tests cover the cache round trip, the search-path and schema
      refusals, a leaf reading its own cache and not a foreign one, and the
      image names.
  - **zsh and fish:** `just check completion-glue` (needs `cli`, `zsh`,
    `fish`) drives both shells' real glue for board keys and RMW names, with
    a negative control on every run. `zsh` and `fish` are new capabilities,
    claimed by `lane.fast`. The shells reach the CI images through
    `ci/docker/apt-packages.txt` (hand-written, COPYed by every CI image) in
    a PR of their own. W4b opens only after the images are rebuilt with
    them (user-approved 2026-11-21), so lane fast never runs the gate on an
    image without the shells.
- **W4c, profiles, codes and toolchains:** done locally (2026-11-21),
  stacked on W4b. All three come from compiled-in tables or one directory
  listing:
  - `nros build --profile=` offers the cargo profiles a CMake build type maps
    to and the four CMake spellings. A test holds that `nros build` accepts
    each one offered.
  - `nros explain` offers every live `NROS-NNNN` code with its title (not
    the retired ones).
  - `nros toolchain uninstall` offers the installed `toolchains/<version>`.
- **W4d, interface types:**
  - The listing moved out of the verb into `interface_index` (`load`,
    `type_names`), which `nros interface` and configure both call, so the
    planning set does not grow by a leaf verb (`check-cli-planning-set` OK).
  - `nros configure` writes the types into the completion cache
    (`interfaces`, `#[serde(default)]`, so a cache written before it reads as
    none and the schema stays 1). An index that does not load writes none
    and never fails the configure.
  - `nros interface show <Tab>` reads them. No cache, no names.
  - Measured on 139: the index load adds 0.11 s to a configure; a configure
    of `examples/linux/rust/talker` with ROS sourced cached 886 types, and
    `nros interface show std_msgs/msg/St<Tab>` offered `std_msgs/msg/String`
    in under 10 ms.

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

- **Status:** OPEN. W5a merged in #4443 (c0a964c08e) and W5b in #4527
  (c80f3de04d), both 2026-11-21; W5c is open: `interface`, `toolchain list`
  and `nros list` have no `--json` schema on main (audited 2026-11-26).
- **W5a, the frame:** `cmd::frame` renders a `list` table, a `show`'s
  fields and Book line, the `--json` document, and the unknown-name refusal
  with the nearest names. The Book URL is the CLI workspace's
  `documentation` field, which `build.rs` compiles in, and a test checks
  `book/book.toml`'s new `site-url` against it. `rmw` and `knob` adopt it.
- **W5b, the board verbs:** `board list` lists board keys, with the
  package (the nearest directory holding a `Cargo.toml` or `package.xml`),
  platform, status and cadence; `board show` adds every table of the
  board's `nros-board.toml`, walked as written, so a new section prints with
  no CLI edit (a fixture test). Both take `--json` (`nros.board.list.v1`,
  `nros.board.show.v1`).
- **Finding:** RFC-0133 D2 labels each descriptor field "from the descriptor
  schema's own field documentation", but no such schema exists: the fields
  are documented only in `board_descriptor.rs` doc comments, written for
  contributors and not readable at run time. `board show` prints the keys as
  the file writes them, the user spelling. Labels wait on a user-facing
  descriptor schema, which needs its own item.
- **W5c, open:** `interface`, `toolchain list` and `nros list` adopt the
  frame and gain `--json`; a contract test per schema.

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

- **Status:** OPEN. W6a and W6b merged in #4529 (e1d26b3c2f, 2026-11-21);
  what is left is `nros profile list`/`show`, which waits on W8 (audited
  2026-11-26).
- **W6a, `nros code` and `nros template`:**
  - `nros code list [--all] [--json]` / `show <code> [--json]` read the
    compiled-in code table (`nros.code.list.v1`, `nros.code.show.v1`).
    `show` prints the explanation `nros explain <code>` does, which stays,
    and links the code's own section of the Book's code page (mdBook's slug
    of its `## NROS-NNNN: <title>` heading; a test reads that heading).
  - `nros template list` / `show <role>` read `node_role::Role::ALL`: role,
    interface package, summary, the component's node name and the three
    languages (`nros.template.list.v1`, `nros.template.show.v1`).
  - Both are leaf verbs of the planning set. The tests hold each list equal
    to its table, so a row added to the table is listed with no CLI edit; a
    compiled-in table takes no fixture row at test time, so that stands in
    for the acceptance's "adds a fixture item".
- **W6b, `nros platform` and `nros runner`:**
  - `nros platform list` / `show <platform>` read `PlatformsTree` over the
    default search path (`$NROS_PLATFORMS_DIR` first): name, aliases,
    `inherits`, capabilities and arch profiles, the catalog boards whose
    platform resolves there, and the RMWs `nros rmw list` verifies on one of
    those boards (`nros.platform.list.v1`, `nros.platform.show.v1`).
  - `nros new --platform` keeps its own words (`linux`, `baremetal`): they
    name what the scaffold emits, not a descriptor. Each platform row shows
    the word that scaffolds on it, derived from `PlatformKind` (its
    descriptor name and its `cmake_deploy` word), and a test holds every
    offered word to a listed platform. So the two lists cannot drift, with no
    second table.
  - `nros runner list` / `show <name>` list the runners (a new
    `extension::discover_runners`, resolving each name as `nros run` does)
    and the extension commands (`extension::discover`), with the
    `[image.<id>.run|flash] method` line that selects each runner
    (`nros.runner.list.v1`, `nros.runner.show.v1`). Runner descriptions are
    not printed: they are contributor notes.
  - Acceptance: a test adds a platform on the search path, and another
    copies the fixture runner and command packages into a workspace; each
    sees its item listed.
  - `PlatformsTree::declared_names` is new: the aliases a descriptor
    declares.
- `nros profile list` / `show` is with W8: the top-level `nros profile` is
  the hidden plumbing cmake and bash call (`args`, `dir`, `env`), and W8
  moves it to `nros dev profile` first.

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

- **Status:** DONE (2026-11-24). Issue 2667's probed Book step on the
  Eigen talker stays with that issue.
- **W7a, `nros image list` / `show`:**
  - `list` reads each bringup's `[image.*]` declarations (discover,
    `collect_images`, `all_images`) and never plans, so it works before
    anything is configured. Columns: image (bare, or `<bringup>:<id>` when
    two bringups declare the id), bringup, board, RMW, launch file.
  - `show <image>` adds what `plan_query` resolves, writing nothing:
    platform, entry package, profile, capabilities, and the packages the
    image builds when the plan states them.
  - Both take `--json` (`nros.image.list.v1`, `nros.image.show.v1`). `show`
    ends with a Book line, and an unknown name is refused with the nearest
    ones.
  - `image` joins the Workspace group of `nros --help`.
- **W7b, `nros image check --no-alloc` (issue 2667), done 2026-11-22:**
  - The reader is Rust, `nros-cli-core/src/alloc_scan.rs`: the four
    families, the whole-name rule, the funnel and the platform-object
    allowances of the old `scripts/check-no-alloc-image.py`. It reads the
    symbol table itself with `object` (already in the lock), ELF and
    archive members alike, so no `nm` is needed. Mangled names are read
    through to their last segment (Itanium, Rust legacy and v0), as `nm -C`
    did.
  - The kernel-allocator naming scheme lives there once
    (`KERNEL_ALLOC_SCHEME`). `check-no-direct-kernel-alloc.sh` reads its
    `grep -E` form from `scripts/lib/kernel-alloc.ere`, generated from it
    and held equal by a test (`NROS_UPDATE_GOLDEN=1` regenerates).
    `scripts/lib/kernel_alloc.py` is gone.
  - `nros image check [<image>] --no-alloc [--tier heap-free|unified]
    [--elf <path>…] [--objects <path>…] [--allow 'SYM=<why>'] [--json]`
    reads the image `nros run` would run (`run_plan::select_target`, and
    `run_plan::build_elf`, now shared with `nros run`'s `elf`). Exit 1 on a
    finding, 2 when it cannot check (`nros.image.check.v1`).
  - The L3 lane's two heap-free runs call the verb with `--elf`. The
    Python script keeps only `--claims`, the Book roster, which reads the
    lane's new spelling, and refuses an image argument by naming the verb.
  - Parity, against the old script on the same files: the heap-free Rust
    and C++ images clean, and the `nros` binary's 16 findings the same 16.
  - Left for issue 2667: its acceptance's probed Book step on the Eigen
    talker. The own-library page names the verb in prose for now.
- **W7c, done 2026-11-24:**
  - `nros image show <image> --nodes` prints the nodes the plan starts
    (PLANNED NODE, PACKAGE, RUNS), and `--topics` the topics, services and
    actions with the nodes on each side, from the model the last `nros
    configure` resolved, read through `model_gate::open`, which resolves
    nothing. No model names `nros configure`; a stale one gives the gate's
    refusal and the same remedy; an image that runs its own entry package
    says no launch file plans it. `--json` adds `planned_nodes` /
    `planned_endpoints` to `nros.image.show.v1`.
  - Measured on a copy of `examples/linux/rust/talker`, configured: `/talker`
    is listed, and `--topics` finds none, because the model carries wiring
    only where a contract declares it.
  - Part 2, done: `--topics` also reads the image's sizing descriptor, at
    W3's path (`ResolvedBuild::sizing_descriptor`, no second rule), behind
    the same staleness gate as `--derived`. Each `[[endpoint]]` row the
    plan does not wire is added; the descriptor names no node, so its side
    reads `image <name>` and the row carries `source = "descriptor"` in
    `--json` (`"plan"` for the model's rows). An image that runs its own
    entry package has no plan, so its descriptor alone answers, and with
    none it names `nros configure`.
  - Measured on a fresh configured copy of `examples/linux/rust/talker`:
    `nros image show linux --topics` lists `/chatter`, topic,
    `std_msgs/msg/String`, FROM `image linux`; `--nodes` still lists
    `/talker`.
  - Tutorial probe step, done: Tutorial 10 (`talk-to-ros2.md`, step 1045)
    runs `nros image show freertos --nodes --topics` on the running image,
    then `ros2 node list` and `ros2 topic list`. Measured on the installed
    track in all three languages (tutorial-10, tutorial-c, tutorial-cpp,
    each PROBE PASS): the six planned nodes are the six on the graph, and
    the planned topic `/sensor/reading` is listed. The C run first showed
    each action's `_action` endpoints from the descriptor; `--topics` now
    leaves out hidden names, as `ros2 topic list` does. C's descriptor
    also records `/parameter_events`, so its table has that row; the page
    shows each language's output in its own tab.
  - Found on the way: the probes' closing `nros board show` check looked
    for a `status` row, which W5b renamed `support` (fixed); and Tutorial
    8's size steps named the debug binary a Rust board image no longer
    builds (paths fixed; its RAM figures are held for the sizing owner,
    because `bss` measured higher after the contract in all three
    languages).

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
