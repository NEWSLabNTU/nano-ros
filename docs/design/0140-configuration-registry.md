---
rfc: 0140
title: "One configuration registry: knobs, Kconfig, carriers and environment variables"
status: Draft
area: workspace-system-model
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-581]
amends: [rfc-0103, rfc-0049, rfc-0071, rfc-0133]  # 0103 D2b + D6 naming; 0049 central schema; 0071 D18 closed Zephyr set; 0133 knob catalog source
supersedes: []
superseded-by: null
---

# RFC-0140 - One configuration registry: knobs, Kconfig, carriers and environment variables

**Status:** Draft, ACCEPTED by the user 2026-11-19 (design approved; work
starts at phase-581 W0). Nothing in Part A is built. The first draft
of this RFC (same day) covered only ROS 2 environment variables; the user
ruled its open questions Q2 to Q11 and reopened Q1 with a wider scope (see
"Decisions"). This revision answers Q1: environment variables and build knobs
are designed together, as rows of ONE registry with many contributors and ONE
naming rule. The first draft's environment-variable design survives as
Part B. Tracked by [phase-581](../roadmap/phase-581-ros-env-vars.md).

Every count below was measured on `origin/main` at `365ad8d9e6` with
`git grep` / `git show` unless the text says "not measured".

## Summary

nano-ros states one configuration fact under up to five names: a
`system.toml` key, an `NROS_*` build carrier, a `CONFIG_NROS_*` Kconfig
symbol, a cmake variable, and sometimes a run-time environment variable.
RFC-0103 D2b and D6 made the build knobs mechanical from one registry row,
but kept 14 Kconfig symbols and 15 three-level TOML keys as exceptions, and
the registry covers neither the 68 hand-written Zephyr Kconfig settings nor
any run-time variable. A board or RMW package that lives outside this tree can
build with no core edit (RFC-0071 D16, RFC-0135), but cannot declare one knob
or one environment variable of its own.

This RFC decides:

1. **One registry, three row kinds** (D1): `knob` (build time), `statement`
   (a `system.toml` selection such as `domain_id`), `env` (run time). One
   namespace across all three.
2. **Many contributors** (D2): core keeps `knobs.toml`; every board, platform
   and RMW package ships its own rows in `nros-knobs.toml` beside its
   descriptor, under its own owner word. Rows are merged when the catalog
   loads; a collision is refused there, naming both files.
3. **One naming rule** (D3, replacing RFC-0103 D2b's exceptions): a knob is
   `<owner>.<name>`, two segments; every other spelling is mechanical;
   sizes end `_bytes`; RMW-, platform- and board-specific rows carry their
   package's owner word; a build name and a run name are never the same.
4. **Renames now** (D4): 87 settings break the rule today (Appendix A). Each
   is renamed, and its old spelling is refused on every road, naming the
   new one. Retired spellings live on the row that replaced them, which folds
   today's `legacy` column and `nros::retired_env::RETIRED_ENV_VARS` into one
   list.
5. **Generated views for contributors** (D5): in-tree rows keep their
   committed views; an out-of-tree package's rows are rendered per build
   into `<build>/nros/` (Kconfig through the `osource` the module already
   has, cmake through an include, Rust and C through a catalog file and the
   package's own build script).
6. **Self-contained packages and an open RMW set** (D6): no central file is
   edited to add a package's knobs, env rows, or (on Zephyr) RMW choice
   member.
7. **Hand-kept lists fold in or are exempted with a reason** (D7), held by a
   gate.

Part B keeps the first draft's environment-variable rule (env over
statement over default on a process with an environment, warn on mismatch,
IGNORE with a notice, REFUSE where unsafe), now expressed as `env` rows.

## Motivation

### The worked example: issue 2714

Issue 2714 found that a `linux` image dropped the `domain_id` and `locator`
its `system.toml` states. PR #4285 fixed it with one Rust decision
(`nros::env::env_overrides_statement`) reached from both language roads, and
a warning when the environment overrides the statement. The first draft of
this RFC generalised that to every environment variable. The user then
widened the question: an environment variable is a configuration name like
any other, and nano-ros has too many of those, in too many places, spelled
too many ways.

### What the user ruled (2026-11-19)

1. Environment variables are designed together with knobs: one source of
   truth and one naming scheme across `system.toml` keys, `NROS_*` build
   carriers, `CONFIG_NROS_*` Kconfig symbols and run-time variables. The
   earlier knob RFCs are read too (RFC-0004, RFC-0049, RFC-0098, RFC-0103,
   RFC-0109, RFC-0114, RFC-0133, RFC-0135, RFC-0071).
2. A setting that breaks the naming rule is RENAMED now; the old name is
   refused at build time, naming the new one (the phase-484 W8 pattern).
3. A custom board package and a custom RMW package are self-contained,
   colcon-style packages that declare their own knobs and environment
   variables without editing central in-tree files.

### Where configuration names live today

| Place | Count on `origin/main` | Generated from a registry? |
| --- | --- | --- |
| `packages/tooling/nros-zephyr-build/knobs.toml` | 112 `[[knob]]` rows: 105 public, 5 carrier, 1 internal, 1 retired; 54 name a `toml` key, 14 a `kconfig` symbol that is not `CONFIG_NROS_<id>`, 32 a `legacy` spelling | It IS the registry (RFC-0103 D2b) |
| Committed views of it | `knob_registry.rs` (`KNOBS`), `cmake/NanoRosKnobRegistry.cmake`, the `zephyr/Kconfig` block between `# >>> knob registry >>>` and `# <<< knob registry <<<` (lines 1387 to 2981, 73 `config` entries), `zephyr/Kconfig.knob-prompts`; plus `knob_road_coverage.tsv` (111 rows, written by a test) | Yes |
| `zephyr/Kconfig` outside that block | 77 `config` entries: 8 structural (the `NROS` menu, 3 `NROS_API` and 3 `NROS_RMW_BACKEND` choice members, Zephyr's own `LINKER_SORT_BY_ALIGNMENT`), 1 path (`NROS_CODEGEN_TOOL`), 68 settings | No |
| `integrations/nuttx/Kconfig` | 10 `config` entries, all menus and choice members (RMW, ROS edition, API) | No |
| `nros-platform-config/src/platform_config.rs` | 56 typed `Option<_>` fields in 11 tenant structs; 54 restate a registry row's `toml` key, 2 (`memory.heap_bytes`, `memory.app_stack_bytes`) have no row; `memory_env_key_for` maps those 2 to 4 platform spellings in two units (`NROS_FREERTOS_HEAP_KB`, `NROS_ZEPHYR_HEAP_SIZE`, ...) | Partly: a test holds the 54 equal to the `toml` column |
| cmake cache entries and options named `NROS_*` (every tracked `*.cmake` and `CMakeLists.txt` outside `third-party/`) | 52 user-settable names: `set(... CACHE <type>)` with a type other than `INTERNAL`, and `option()`. phase-581 W0 counted them (the first count, 31, had a narrower scope) and classified them: 16 row, 16 resource, 20 plumbing | Some (the `NROS_KNOB_*` ones are the cmake registry view) |
| Build-script reads (literal `rerun-if-env-changed=` lines, outside `packages/cli`) | 46 nano-ros-shaped names (`NROS_*`, `ZPICO_*`, `XRCE_*`, `CYCLONE*`); 22 are not registry rows or legacy spellings (`NROS_LOCATOR`, `NROS_DOMAIN_ID`, `NROS_ZPICO_DEBUG`, `ZPICO_NO_SMOLTCP`, `ZPICO_MULTICAST_TRANSPORT`, `NROS_LINK_IP`, `NROS_BOOT_REPORT`, the `NROS_DECLARED_*` and `NROS_ENTITY_*` carriers, ...). A lower bound: reads through the `knob()` helper are not literal | No |
| Run-time environment reads | about 25 names (Appendix B); `NROS_ENTRY_SPIN_MS` alone has 7 readers with 4 spellings of the read | No registry |
| Retired run-time names | `nros::retired_env::RETIRED_ENV_VARS`, 2 rows (`ZENOH_LOCATOR`, `ZENOH_MODE`) | A separate list |
| Zephyr RMW set | `ZEPHYR_MODULE_BACKENDS = ["zenoh", "xrce", "cyclonedds"]` (`rmw_descriptor.rs`) and the hand-written `choice NROS_RMW_BACKEND` | Closed by RFC-0071 D18 |

### What the measurement says

1. **The mechanical rule has holes.** 30 of 112 registry rows have an env
   stem that is not the mechanical form of their TOML key: 13 because the
   key has three segments (`zenoh.wire.frag_max_size` is `NROS_ZENOH_FRAG_MAX_SIZE`,
   `zenoh.limits.service_timeout_ms` is `NROS_SERVICE_TIMEOUT_MS`), 17
   because the stem's group differs from the key's (`params.max_parameters`
   is `NROS_MAX_PARAMETERS`; `net.max_sockets` is `NROS_SMOLTCP_MAX_SOCKETS`).
   14 more carry a Kconfig symbol that is not the stem (RFC-0103 D2b kept
   them so no guest `prj.conf` broke).
2. **Units are spelled five ways.** `memory.heap_bytes` (bytes),
   `NROS_FREERTOS_HEAP_KB` (KiB), `NROS_ZEPHYR_HEAP_SIZE` (bytes),
   `NROS_EXECUTOR_BACKING_U64S` (8-byte words); `_SIZE`, `_LEN` and
   `_BUF_SIZE` stand for bytes in 26 more registry rows. A queued PR (#4355, not read here) adds
   `THREADX_BYTE_POOL_KB`.
3. **An owner word is missing or doubled.** Zenoh-only rows with no
   `ZENOH_` (`KEYEXPR_STRING_SIZE`, `GRAPH_MAX_ENTITIES`); Cyclone spelled
   `CYCLONE`, `CYCLONEDDS` and `RMW_CYCLONEDDS`; parameters spelled `PARAM`
   and `PARAMS`; the smoltcp rows spelled `net` in TOML and `SMOLTCP` in env.
4. **Build and run share names.** `NROS_LOCATOR` is a run-time variable on a
   Linux process (`nros::env::env_rung`) and a build-time `option_env!` in
   `nros::init::Context::baked` and two board configs (`mps2-an385`,
   `nuttx-qemu`); its build-time sibling `NROS_DOMAIN_ID` is read by
   `option_env!` in five. `NROS_NODE_NAME`
   likewise (`nros-board-mps2-an385/src/rtic.rs`). `ENTRY_SPIN_MS` is a
   registry knob (class `internal`) whose only readers by name are 7
   run-time `getenv`s.
5. **A package cannot declare a row.** A board's `[knobs]` accepts only
   registry rows (issue 2194's `Knobs::registry` map is keyed on them); the
   one open map, `Knobs::backends`, accepts only `[knobs.<backend>.tx]`. An
   RMW descriptor has no knob or env field. To add a knob a package edits
   `knobs.toml`, re-runs the generator over four committed views, edits the
   typed tenant struct, and on Zephyr edits the module `Kconfig` choice and
   `ZEPHYR_MODULE_BACKENDS`. RFC-0049 called a central file "the
   out-of-tree wall"; for knobs the wall is still standing.

## Part A - The registry

### D1 - One registry, three row kinds, one namespace

Every configuration name nano-ros defines or reads is a row of one registry.
A row is one of three kinds, and the kind fixes its phase:

| Kind | Phase | What it is | User spelling | Derived spellings |
| --- | --- | --- | --- | --- |
| `knob` | build | A sizing, policy or feature value with a built-in default and the RFC-0049 ladder (builtin < platform < board < image) | `[knobs.<owner>] <name>` in a platform or board descriptor; `[image.<id>.knobs.<owner>] <name>` in `system.toml` | build env `NROS_<OWNER>_<NAME>`, Kconfig `CONFIG_NROS_<OWNER>_<NAME>`, cmake override `-DNROS_OVERRIDE_<OWNER>_<NAME>` |
| `statement` | build | A `system.toml` selection: `domain_id`, `locator`, `rmw`, and the launch-declared `node_name` and `node_namespace` | `[system]` / `[image.<id>] <key>`, or the launch file | build carrier `NROS_ENTRY_<KEY>`; Kconfig `CONFIG_NROS_<KEY>` where the Zephyr module consumes the statement itself |
| `env` | run | A variable a running process reads | the variable itself | none |

The schema, in the files D2 names:

```toml
owner = "zenoh"                 # the file's one owner word (D2)

[[knob]]
key = "zenoh.frag_max_bytes"    # <owner>.<name>; every spelling derives from it
scope = "image"                 # RFC-0133: image | board | platform
class = "public"                # RFC-0133: public | carrier | internal | retired
type = "int"
prompt = "Maximum reassembled message size (bytes)"
kconfig_if = "NROS_RMW_ZENOH"   # framework condition, as today
retired = [                     # D4: every old spelling this row replaces
  "toml:zenoh.wire.frag_max_size",
  "env:NROS_ZENOH_FRAG_MAX_SIZE",
  "env:ZPICO_FRAG_MAX_SIZE",
  "kconfig:CONFIG_NROS_FRAG_MAX_SIZE",
]

[[env]]
name = "ZENOH_SESSION_CONFIG_URI"
foreign = "zenoh"               # defined by another project; never renamed
disposition = "respect"         # Part B: respect | ignore | refuse | listed
stated = "rmw_config"           # the statement it is compared with
parse = "str"
```

`[[statement]]` rows exist only in core's file (the `system.toml` schema is
core's). `id`, `toml` and `kconfig` stop being authored: `key` is the one
authored name, and the generator refuses a row that states a spelling D3
would derive. RFC-0133's catalog fields (`scope`, `class`, `prompt`, `help`,
the derived `default`, `values`, `surface`) are unchanged.

**One namespace.** Every spelled name of every row (TOML key, build env,
Kconfig symbol, cmake override, run-time variable, and every retired
spelling) is unique across the merged registry. The loader checks it; a
second use is a load error naming both rows (D2 rule C3).

**Why kinds, not one shape.** A statement is not a knob: it has no ladder
(the platform and board never state a domain), and its build carrier is
`NROS_ENTRY_<KEY>`, which already exists for the two that have one
(`NROS_ENTRY_LOCATOR`, `NROS_ENTRY_DOMAIN_ID`, read by
`nros-board-mps2-an385/src/config.rs`). A run-time variable has no carrier at
all. Folding the three into one row shape would leave most fields empty on
most rows; three kinds in one namespace keeps one name per fact and one
loader.

### D2 - Owners: who may declare a row

**The core file stays** (`packages/tooling/nros-zephyr-build/knobs.toml`).
It declares the `[[statement]]` rows, the core knob and env rows, and the
foreign env rows of ROS 2 itself (`ROS_*`, `RMW_IMPLEMENTATION`,
`RCUTILS_*`) and of Rust (`RUST_LOG`). It lists the core owner words in an
`[owners]` table, one line of description each. The list starts as the
first segments core rows use today, in the registry and in the hand Kconfig
and run-time reads D7 folds: `action`, `arena`, `bless`, `boot`,
`bridge`, `capability`, `census`, `console`, `declared`, `entity`, `entry`,
`executor`, `fault`, `graph`, `heap`, `init`, `link`, `log`, `memory`,
`monitor`, `net`, `params`, `profile`, `pubsub`, `rmw`, `runtime`,
`service`, `session`, `sntp`, `subscriber`, `trace`, `violation`. Adding a core owner word is a breaking change and is
named in the changelog, because a downstream package may already hold it.

**A package file.** A board, platform or RMW package declares its rows in
`nros-knobs.toml` beside its descriptor (`nros-board.toml`,
`nros-platform.toml`, `nros-rmw.toml`). The file is found by the scan that
already finds the descriptor: the board catalog's roots (RFC-0135 D5), the
platform search path (RFC-0049, RFC-0086), the RMW provider scan (RFC-0071
D5). No descriptor gains a field for it, because a field would be a second
place naming the same file; the file's presence is the declaration, and its
absence means "no rows".

**The owner word is derived, not chosen:**

- an RMW package: its RMW name, the word an image writes in `rmw = "<name>"`
  (`zenoh`, `xrce`, `cyclonedds`, `uorb`, `acme`);
- a platform package: its platform name (`freertos`, `threadx`, `zephyr`,
  `nuttx`, `posix`);
- a board package: one of the board keys it defines, with `-` read as `_`
  (`nucleo-f767zi` gives `nucleo_f767zi`). A package that defines several
  keys names which one in its `owner` line; the loader checks that it is one
  of them.

The file's `owner` line must equal the derived word; it is stated so the file
reads on its own, and checked so it cannot drift.

**Collision rules, all checked when the catalog loads** (every `nros`
command that loads it: `configure`, `build`, `knob list`, `knob show`):

- **C1 - one owner, one package.** Two different packages with the same
  owner word are refused, naming both files and their roots. The SAME
  package found in two roots is not a collision: RFC-0135 D5's precedence
  picks the first and reports the shadowed copy (identity is the package
  name, RFC-0087).
- **C2 - a row stays under its owner.** A package row whose key does not
  start with the file's owner word, or a core row whose owner is not in
  `[owners]`, is refused. A package cannot declare a core owner's row, and
  core cannot take a package's word.
- **C3 - one spelling, one row.** Any spelled name used twice (including a
  retired spelling, a foreign variable, and a resource variable from
  `nros-sdk-index.toml`, RFC-0103 D6) is refused, naming both rows.
- **C4 - foreign rows have one declarer.** A foreign variable is declared
  by core (ROS 2, rcutils, Rust) or by the package whose library defines it
  (`ZENOH_*` by the zenoh package, `CYCLONEDDS_URI` by the cyclonedds
  package). A second declarer is C3.

**What core may read.** Hand-written core code never reads a
package-owned name. A generated view is not a reader: the Zephyr module's
generated forwarding of `CONFIG_NROS_ZENOH_*` into cargo's environment
(today `zephyr/cmake/nros_cargo_build.cmake`) is rendered from the rows and
stays in core. The gate (D9 G3) holds this; today one row breaks it
(`SERVICE_TIMEOUT_MS`, TOML key `zenoh.limits.service_timeout_ms`, is also
read by `nros-c`). W0 measured it a core fact: one resolver
(`resolve_service_timeout_ms`) feeds both the zenoh backend's constant and
`nros-c`'s default client timeout, which every C service client uses whatever
the RMW. It becomes `service.timeout_ms`, whose env spelling
`NROS_SERVICE_TIMEOUT_MS` is already the mechanical one ([W0 data](../roadmap/data/phase-581-w0-measure.md),
section 8).

**The in-tree packages move their rows out.** zenoh, xrce, cyclonedds,
freertos, threadx and zephyr rows leave the core file for their packages'
`nros-knobs.toml`. This is the proof that the contributor path works; the
committed views are rendered from the merged in-tree set and stay
byte-identical apart from row order.

### D3 - The naming rule (amends RFC-0103 D2b and D6)

This replaces the "Knobs" bullet of RFC-0103 D6 and the "the Kconfig symbol
is a COLUMN, not the id" paragraph of D2b. RFC-0103's "no pairs table, no
aliases" stands and now has no exceptions.

- **N1 - Two segments.** A knob's key is `<owner>.<name>`: exactly two
  segments, lower snake case, `<owner>` an owner word from D2. A TOML
  sub-table (`[knobs.zenoh.tx]`, `.wire`, `.limits`) is a third segment and
  is not allowed. Grouping for display is the row's `help` and RFC-0133's
  listing, not its name. Reason: the first segment is what makes ownership
  checkable (C2), and a third segment either leaks into every spelling
  (`CONFIG_NROS_ZENOH_WIRE_FRAG_MAX_SIZE`) or is silently dropped from some,
  which is how 13 of today's 30 mismatches arose.
- **N2 - Every other spelling is mechanical.** From key `a.b`: build env
  `NROS_A_B`, Kconfig `CONFIG_NROS_A_B`, cmake override
  `-DNROS_OVERRIDE_A_B`, image TOML `[image.<id>.knobs.a] b`. A statement's
  carrier is `NROS_ENTRY_<KEY>` and its Kconfig symbol `CONFIG_NROS_<KEY>`.
  A C macro inside a backend (`ZPICO_*`, `XRCE_*`) is generated by that
  backend's build and is not a configuration name; it is out of scope of the
  rule, and it is never read from the environment.
- **N3 - The owner is whoever's code it configures.** A row whose readers
  are all in one RMW, platform or board package is owned by that package,
  so its name carries that package's word (`zenoh.keyexpr_string_bytes`, not
  `KEYEXPR_STRING_SIZE`). A row read by core code is a core row, even when
  only one backend's Kconfig section shows it today: `SERVICE_INBOX_BYTES` is
  read by `nros-node`, so it stays `service.inbox_bytes`. One subject has one
  owner word: `params`, not `param`; `cyclonedds`, not `cyclone` or
  `rmw_cyclonedds`; `net`, not `smoltcp`.
- **N4 - Units.** A memory size ends `_bytes` and its value is in bytes. A
  duration ends `_ms` or `_us`. A count has no unit suffix; `_depth` is a
  queue's entries, `_len` is a number of elements (never bytes). A short
  allowlist in the lint holds terms whose unit the term fixes: `mtu` (bytes),
  `baud`, `frames`. Banned endings: `_kb`, `_kib`, `_size`, `_buf_size`,
  `_buffer` with no unit, `_u64s`. A carrier that a platform needs in
  another unit (FreeRTOS `configTOTAL_HEAP_SIZE`, a KiB stack) is converted
  at the one place that writes the RTOS configuration, never in the name.
- **N5 - A build name is never a run name.** No spelled build name (knob or
  statement carrier) equals a spelled `env` row. A run-time override of a
  statement is `NROS_<KEY>` (`NROS_LOCATOR`, `NROS_RMW`, `NROS_NODE_NAME`)
  or the foreign variable (`ROS_DOMAIN_ID`); the statement's build carrier is
  `NROS_ENTRY_<KEY>`. Any other nano-ros `env` row is `NROS_<OWNER>_<NAME>`
  from a two-segment key, like a knob, and the uniqueness check (C3) keeps it
  apart from the build rows.
- **N6 - One prefix.** Every name nano-ros defines starts `NROS_` (env and
  cmake) or `CONFIG_NROS_` (Kconfig). A name another project defines
  (`ROS_*`, `RMW_*`, `RCUTILS_*`, `RUST_LOG`, `ZENOH_*`, `CYCLONEDDS_URI`) is
  a `foreign` row, never renamed, and nano-ros invents no name in another
  project's prefix: the six `ZENOH_*` variables nano-ros invented are retired
  (Decision 4).
- **N7 - A spelling is never reused.** A retired spelling stays in the
  registry for good (D4), so C3 refuses a new row that would take it.
- **N8 - A resource location is not a setting.** Its one variable is the
  `env` of its row in `nros-sdk-index.toml` (RFC-0103 D6:
  `NROS_HOME`, `FREERTOS_DIR`, ...). The lint reads the index so a setting
  cannot take a resource's name, and resource variables are outside N1 to N4.

**What the rule costs.** Appendix A lists every setting on `origin/main`
that breaks N1 to N6: **87**. 54 are registry rows (52 renamed, 2 retired),
12 are hand-written Zephyr Kconfig symbols, 9 are run-time variables (3
renamed, the 6 retired `ZENOH_*` of Decision 4), 6 are build carriers outside
the registry, and 6 are the demand symbols RFC-0103 D2b kept as a column
(staged last, see below). 45 of the 54 registry rows change a TOML key a
user writes in `system.toml` or a descriptor; the rest of the 87 change only
carriers, Kconfig symbols and run-time variables.

**The demand symbols.** RFC-0103 D2b measured that the six zenoh pools
(`ZENOH_MAX_PUBLISHERS`, `_SUBSCRIBERS`, `_QUERYABLES`, `_LIVELINESS`,
`_PENDING_GETS`, `_LARGE_SUBSCRIBERS`) read the RMW-agnostic DEMAND symbol
`CONFIG_NROS_MAX_*`, while the env carrier is the pool after its floor
(issues 1015/1033). That is two facts sharing one Kconfig name, which is why
D2b needed a column. Under N3 they are two rows: a core `entity.max_<x>`
(the demand, `CONFIG_NROS_ENTITY_MAX_<X>`, replacing the
`NROS_MAX_<X>` cmake carriers of `zephyr/cmake/nros_cargo_build.cmake`) and
the zenoh pool `zenoh.max_<x>` (class `carrier`, derived from the demand
through the floor, no Kconfig symbol). W0's census lists every reader by
file and line ([W0 data](../roadmap/data/phase-581-w0-measure.md), section 7): the `NROS_MAX_<X>` cmake carriers
exist on the Zephyr road only; `cmake/NanoRosMessageBounds.cmake` and
`cmake/NanoRosCodegenCore.cmake`, which the first draft named, only mention
the name in comments and messages. The split is its own stage (phase W7).

### D4 - Retired spellings live on the row that replaced them

A row's `retired` list holds every spelling it replaced, tagged by road:
`toml:`, `env:`, `kconfig:`, `cmake:`, `run:`. A unit change carries its
factor, so the refusal can say what to write:

```toml
retired = [{ spelling = "env:NROS_FREERTOS_HEAP_KB", factor = 1024 }]
```

A spelling retired with no replacement row (today `ZENOH_LEASE_PRIORITY`,
class `retired`) stays a `class = "retired"` row with an `instead` text.

This is the one list. Today's 32 `legacy` spellings fold into it, and so do
the two rows of `nros::retired_env::RETIRED_ENV_VARS`, which become the
`retired` entries of the `NROS_LOCATOR` and `NROS_SESSION_MODE` env rows.
`RETIRED_ENV_VARS` becomes a generated view, so the gate
`check-retired-runtime-aliases` keeps its subject.

**Where a retired spelling is refused, by road.** Each check reads the
merged registry, so a package's retired spellings are refused like core's:

| Road | Who refuses | What the user sees |
| --- | --- | --- |
| `toml:` | the knob parser (platform, board, image tables) | `[knobs.zenoh.wire] frag_max_size` is retired: write `[knobs.zenoh] frag_max_bytes` |
| `env:` (build) | `nros configure` / `nros build`, once per configure, over the build shell; and the one ladder (`nros_zephyr_build::knob`) and `cmake/NanoRosKnobEnv.cmake` for a build not started by `nros` | `NROS_FREERTOS_HEAP_KB=64 is retired: set memory.heap_bytes = 65536 (KiB x 1024)` |
| `kconfig:` | the module hook, before Kconfig runs, over the merged conf files and Zephyr's `CLI_CONFIG_*` cache, in a project with a `system.toml` and in a guest without one | `CONFIG_NROS_FRAG_MAX_SIZE is retired: CONFIG_NROS_ZENOH_FRAG_MAX_BYTES` (guest) or the `system.toml` key (managed project) |
| `run:` | the `nros-env` startup sweep (Part B, D11) | the boot stops: `ZENOH_LISTEN is retired: use ZENOH_CONFIG_OVERRIDE='listen/endpoints=[...]'` |

Retirement is permanent (phase-484 W8's ruling of 2026-10-14: an unlisted
old name would be ignored, not refused). The hook refuses a retired Kconfig
symbol itself, naming the replacement. **M6 (W0, measured):** without the
hook, Zephyr 3.7 and 4.4 both print `warning: attempt to assign the value ...
to the undefined symbol ...` and then `error: Aborting due to Kconfig
warnings`, and configure fails with no `.config`. So a guest is never
silently given the default, but Zephyr's message names no replacement; the
hook's refusal is what does ([log](../roadmap/data/phase-581-w0-m6-configure.log)).

Two run-time names stay as run-time variables while their build-time
meaning is retired: `NROS_LOCATOR` and `NROS_NODE_NAME`. A build that finds
them in its shell does not refuse (a developer exports them to RUN a hosted
image); for an image with no process environment it prints Part B's D10
note instead. `NROS_DOMAIN_ID` never was a run-time variable, so a build
refuses it, naming `[system] domain_id` and the override channel.

### D5 - Generated views, for in-tree and out-of-tree rows

| View | In-tree rows (core + in-tree packages) | An out-of-tree package's rows |
| --- | --- | --- |
| Rust knob table (`KNOBS` in `knob_registry.rs`) | committed, as today | not in the table. `nros configure` writes the image's merged catalog to `<build>/nros/knobs/catalog.toml` and names it to every road as `NROS_KNOB_CATALOG` (a `relative = true` cargo `[env]` row and a cmake `-D`, the channel the sizing descriptor already uses, RFC-0100 D4). The ladder `nros_zephyr_build::knob()` answers a name the table lacks from that file |
| A package's own Rust readers | n/a | the package's build script calls `nros_knob_build::emit(manifest_dir)`, which reads the package's own `nros-knobs.toml` and writes `$OUT_DIR/nros_knobs.rs` (typed `static` rows, env rows included) for the crate to `include!` |
| cmake registry (`NanoRosKnobRegistry.cmake`) | committed | `<build>/nros/knobs/registry.cmake`, included right after the committed file |
| Zephyr Kconfig | committed block in `zephyr/Kconfig`, plus `Kconfig.knob-prompts` for a guest | `<build>/nros/Kconfig.packages`, written by the module hook and `osource`d by the module `Kconfig` beside the existing `osource "$(KCONFIG_BINARY_DIR)/../nros/Kconfig.from-system-toml"` (line 1404). RFC-0103 D2b measured that shape on 3.7 and 4.4 |
| C header of env names (`<nros/env_rows.h>`) | committed | `<build>/nros/knobs/<owner>_env.h` |
| The image's run-row table (Part B's startup sweep) | n/a | generated into the image's entry, which already lists the image's backends (RFC-0071 D13); it names core rows and the linked packages' rows |
| `nros knob list` / `show`, the book page | the book page is committed (phase-569 W2c) | `nros knob list` reads the merged catalog of the workspace (provider scan, memoised as `build/nros/providers.json` already is), with a package and root column |
| `knob_road_coverage.tsv` | committed, written by its test | not covered: a package's road coverage is its own CI's |

**Limits, stated.**

- A core crate's compiled table can never hold an out-of-tree row; that is
  why the ladder falls through to a file. Compiling package rows into
  `KNOBS` per image was rejected (Alternatives): it makes the build helper's
  fingerprint depend on the image's package set, the 0491 class.
- A Zephyr GUEST project (RFC-0072: no `system.toml`, so no `nros configure`
  and no `Kconfig.packages`) sees no out-of-tree package's knobs through the
  nano-ros module. A package that wants guest support is a Zephyr module of
  its own; `nros knob render --kconfig <package>` writes that module's
  `Kconfig` from its `nros-knobs.toml`, and the package commits it.
- NuttX's `Kconfig` holds only menus and choices; NuttX images take knobs
  through the cargo and cmake carriers, so a package's knobs reach a NuttX
  image the same way as any other road. No NuttX-specific view is designed.

### D6 - Self-contained board and RMW packages; an open RMW set

**Central files a package no longer touches**, each with what replaces it:

| Central file today | Replaced by |
| --- | --- |
| `knobs.toml` | the package's `nros-knobs.toml` (D2) |
| the four committed views | per-build views (D5) |
| `platform_config.rs` typed tenants | the generic `Knobs::registry` map (issue 2194's), keyed by row and validated against the merged catalog, becomes the only parsed shape for platform and board files too. In-tree rows get generated typed accessors, so build scripts keep a typed read. An unknown key is refused against the catalog, which keeps RFC-0049's "unknown keys fail loud" (`deny_unknown_fields` is what cannot see a package's rows) |
| `knob_road_coverage.tsv` | not needed for package rows (D5) |
| `zephyr/Kconfig`'s `choice NROS_RMW_BACKEND` and `ZEPHYR_MODULE_BACKENDS` | generated from the descriptors, below |
| a platform's `[arch.*]` | RFC-0135 D4 (not this RFC) |

**An image states a package's knob** exactly as a core one:
`[image.<id>.knobs.acme] pool_slots = 4`. A key whose owner is not a package
of this image is refused, naming the owner and the packages the image links.

**The open RMW set (amends RFC-0071 D18).** D18's `[rmw.zephyr]
module_backend = "zenoh"` stays for a backend built over an existing module
member (the `acme` fixture). A backend with its own Zephyr code adds:

```toml
[rmw.zephyr]
module = "zephyr"     # the package directory that holds its own module.yml
```

The hook adds that directory to `ZEPHYR_EXTRA_MODULES`, and
`Kconfig.packages` adds the member `NROS_RMW_<OWNER>` to
`choice NROS_RMW_BACKEND`. **M5 (W0, measured on 3.7 and 4.4): green.** A
`choice NROS_RMW_BACKEND ... endchoice` block holding one `config` in another
file, both from an `osource`d `<build>/nros/Kconfig.packages` beside the
existing `Kconfig.from-system-toml` line and from an out-of-tree module's own
`Kconfig`, joins the choice declared with its prompt in `zephyr/Kconfig`: a
conf line selects it, its `select` applies, and with no assignment the
choice's default stays. (RFC-0103's M1 measured a different shape, a
prompt-less redefinition, and found it red.) Kconfig does NOT refuse a conf
that assigns a second member: the last assignment wins, with no warning. So
the hook refuses a conf member that disagrees with the image's `rmw`, and the
`NROS_RMW_EXTERNAL` fallback is not needed
([log](../roadmap/data/phase-581-w0-m5-configure.log)).
`ZEPHYR_MODULE_BACKENDS` stops being a literal: it is the in-tree
members plus the module-providing packages the scan finds, and D18's unit
test compares it with the rendered choice. NuttX's RMW choice
(`integrations/nuttx/Kconfig`) is generated from the in-tree descriptors the
same way; an out-of-tree NuttX RMW is not designed here.

### D7 - Hand-kept lists fold in or are exempted with a reason

| List | Disposition |
| --- | --- |
| 68 settings in `zephyr/Kconfig` outside the generated block | Fold into rows. The generator already renders `kconfig_if` and `kconfig_attrs` (`depends on`, `select`, conditional defaults), which RFC-0049's amendment of 2026-11-14 made the reason a generator is acceptable. 12 of the 68 are renamed in the same stage (Appendix A.2); the rest already satisfy N1 to N6 once their first segment is a declared owner |
| 9 other entries there | Exempt, with the reason in the exemption file: the `NROS` menu and the `NROS_API` choice (structure, not settings); Zephyr's own `LINKER_SORT_BY_ALIGNMENT` (not ours); `NROS_CODEGEN_TOOL` (a path, RFC-0103 D6); the 3 `NROS_RMW_BACKEND` members until D6 generates them |
| 10 entries in `integrations/nuttx/Kconfig` | Exempt (menus and choices); the RMW choice is generated by D6 |
| 56 typed tenant fields | Folded by D6's generic map; the 2 `memory.*` fields get rows (Appendix A.1) and `memory_env_key_for` is deleted |
| 32 `legacy` spellings and `RETIRED_ENV_VARS` | Folded by D4 |
| `Knobs::backends` (`[knobs.<backend>.tx]`, a third segment; empty in every in-tree file per its own doc comment) | Replaced by the backend package's own rows (`[knobs.<backend>] tx_batch`) |
| 52 user-settable cmake `NROS_*` cache entries and options | W0 classified each ([W0 data](../roadmap/data/phase-581-w0-measure.md), section 9): 16 rows, 16 resource locations (RFC-0103 D6), 20 build plumbing (exempt, with the reason). Names that become rows take N2 spellings; candidates seen but not counted in Appendix A: `NROS_RMW_CYCLONEDDS_FOREIGN_WAKE`, `NROS_RMW_CYCLONEDDS_FOREIGN_WAKE_BRIDGE`, `NROS_PLATFORM_FREERTOS_HEAP_3`, `NROS_RUST_MAIN_STACK_FLOOR` (a byte count) |
| 22 build-script env names that are not rows | Each becomes a row (most are carriers, `class = "carrier"`) or a resource location, or is exempt as cargo's or the toolchain's own variable |
| Run-time reads (Appendix B) | Every one becomes an `env` row (Part B) |

## Part B - Environment variables

Part B is the first draft's design, unchanged in substance, with each
variable now an `env` row of D1 and each Q ruled.

### D8 - The rule, and where it applies

For a variable whose disposition is RESPECT, on a process that has an
environment:

```
value = env (when set) > stated (system.toml, as baked) > built-in default
```

The precedence is the existing one (`try_resolve_with`, RFC-0045). When the
environment and the statement both give a value and they differ, the process
warns once:

```
[WARN] [nros.env]: ROS_DOMAIN_ID=7 overrides system.toml's domain_id=42
```

"Stated" means what the image's build inputs say: `system.toml`, its launch
file (node name and namespace) and its `rmw_config` file; never a built-in
default. The `env` row names its statement in `stated`, which is a
`[[statement]]` key or a knob key; the generator refuses a `stated` that is
neither. A process "has an environment" when it is a Linux process with a C
library, or a NuttX process (Decision 9); D10 covers the rest.

### D9 - Disposition per variable

| Variable | Disposition | Reason |
| --- | --- | --- |
| `ROS_DOMAIN_ID` | RESPECT (done by issue 2714) | The user's ruling |
| `NROS_LOCATOR` | RESPECT (done by issue 2714) | Where the image connects |
| `NROS_RMW` | RESPECT, warn on mismatch with `rmw` | An override can pick only among linked backends; a mismatch is worth a line |
| `NROS_NODE_NAME`, `NROS_NODE_NAMESPACE` | RESPECT, warn on mismatch with the launch-declared value | A silent identity override is how a graph gets two nodes of one name. W0: read on all three Linux roads, but a component- or application-named node kept its own name, so today the override reaches only the default node name (W12) |
| `NROS_SESSION_MODE` | RESPECT, warn when the zenoh file states another `mode` | Core's `ExecutorConfig` field, so owner `session`; RFC-0109 puts the mode in the middleware file |
| `ZENOH_SESSION_CONFIG_URI` | RESPECT, warn when the image baked a file | Replaces the baked table whole (RFC-0109 D8) |
| `ZENOH_CONFIG_OVERRIDE` | RESPECT on both roads, lowered by the same reader as the session file; warn when it changes a key the baked file states | Decision 4: the stock spelling for key overrides |
| `ZENOH_MULTICAST_SCOUTING`, `ZENOH_SCOUTING_TIMEOUT`, `ZENOH_LISTEN`, `ZENOH_TLS_ROOT_CA_CERTIFICATE`, `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64`, `ZENOH_TLS_VERIFY_NAME_ON_CONNECT` | REFUSE, each naming the `ZENOH_CONFIG_OVERRIDE` key that replaces it | Decision 4: nano-ros inventions in zenoh's prefix (N6) |
| `CYCLONEDDS_URI` | RESPECT, warn when the image baked a file, saying the two COMPOSE | Cyclone composes them (RFC-0109 D8 amendment 3) |
| `NROS_BRIDGE_<N>_LOCATOR`, `NROS_BRIDGE_<N>_DOMAIN` | RESPECT, warn on mismatch with `[[domain]]` | A patterned row (`pattern = true`) |
| `ROS_LOCALHOST_ONLY`, `ROS_AUTOMATIC_DISCOVERY_RANGE`, `ROS_STATIC_PEERS` | IGNORE, notice naming the middleware file | Decision 2 |
| `RMW_IMPLEMENTATION` | IGNORE as a selector; notice only when it maps to a backend other than the image's | Fixed map `rmw_zenoh_cpp`/`rmw_cyclonedds_cpp`/`rmw_microxrcedds` keeps the common case silent; the `Context.rmw` hint keeps reading it |
| `ROS_NAMESPACE` | IGNORE, notice naming `NROS_NODE_NAMESPACE` | Decision 3: W0 measured stock Humble rcl, rclcpp and rclpy ignoring it (Jazzy not measured); the `ExecutorConfig::namespace` doc already says nano-ros does not read it |
| `ROS_LOG_DIR`, `ROS_HOME` | IGNORE, notice | nano-ros writes no log file |
| `RCUTILS_COLORIZED_OUTPUT`, `RCUTILS_CONSOLE_OUTPUT_FORMAT` | RESPECT in the Linux console sink, on every road with that sink | Decision 5 |
| `RCUTILS_LOGGING_USE_STDOUT`, `RCUTILS_LOGGING_BUFFERED_STREAM` | IGNORE, notice | One sink per board |
| `RUST_LOG` | RESPECT on the Rust road only (the `nros-board-linux` stdout sink, as today) | Decision 5 |
| `ROS_DISABLE_LOANED_MESSAGES` | IGNORE, notice | Decision 6 |
| `ROS_SECURITY_ENABLE` with `ROS_SECURITY_STRATEGY=Enforce` | REFUSE | Running unsecured while the operator believes otherwise is the one unsafe IGNORE |
| other `ROS_SECURITY_*` | IGNORE, notice | No SROS2 |
| `ZENOH_LOCATOR`, `ZENOH_MODE` | REFUSE (unchanged; now `retired` entries of `NROS_LOCATOR`/`NROS_SESSION_MODE`) | phase-484 W8c |
| `ZENOH_ROUTER_CONFIG_URI` | IGNORE, notice | Decision 7 |
| Operational `NROS_*` (`NROS_ENTRY_SPIN_MS`, `NROS_ENTRY_SPIN_STEP_MS`, `NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS`, `NROS_CENSUS_OUT`, `NROS_PROFILE_OUT`, `NROS_PROFILE_INPUTS`, `NROS_GRAPH_DUMP`, `NROS_RMW_TRACE_OPEN`, `NROS_BLESS_PROBE_SIDECAR`), and the package rows `NROS_XRCE_SERIAL_BAUD`, `NROS_CYCLONEDDS_MATCH_TIMEOUT_MS`, `NROS_CYCLONEDDS_SKIP_PREFIX` | Registered, read through D11, no statement rule | Tooling and test controls. `ENTRY_SPIN_MS` stops being a build knob: its only readers by name are run-time (Appendix A.1) |

No new `system.toml` key is proposed: discovery scope, session mode and
middleware tuning are middleware wiring (RFC-0109), and logging format has
no deployment owner.

### D10 - A process with no environment

An embedded image (FreeRTOS, ThreadX, Zephyr on hardware or QEMU, bare
metal) has no process environment; the stated value is the value. Zephyr
`native_sim` has none either (W0, Decision 8: its picolibc `getenv` returns
NULL for every name, on 3.7 and 4.4). NuttX reads its environment
(Decision 9).

A user learns that a variable does nothing from:

1. **Docs.** The book's reference renders the `env` rows, with a "has an
   environment" column per board, read from one board-descriptor fact
   (`process_env = true | false`).
2. **A build-time note.** `nros build` and `nros configure`, for an image
   whose board has no process environment, check the build's shell for every
   RESPECT row and print, once per variable:

   ```
   note: ROS_DOMAIN_ID=5 is set in this shell, but image `talker`
         (board mps2-an385) has no process environment: it uses
         system.toml's domain_id=7. Change system.toml to change it.
   ```

   A one-build override of a statement stays RFC-0103 D2's command-line
   channel (printed, recorded as `transient`). After D4, an exported
   `NROS_DOMAIN_ID` is not part of it: the build refuses it (Appendix A.4).

### D11 - One implementation point for `env` rows

A `#![no_std]` crate `nros-env` (in `packages/core/`, depending only on
`nros-log`) holds the one `getenv` call and the decisions:

- `get(row) -> Option<&'static str>`, compiled to "always unset" where the
  target has no environment;
- `resolve(row, stated) -> Option<T>`: env over stated, with the mismatch
  warning (issue 2714's `env_overrides_statement` and
  `warn_env_overrides_statement`, generalised from two fields to a row);
- `notice_if_set(row)`, `refuse_if_set(row)`;
- a once-per-row atomic bitset, so a row warns at most once per process
  however many sessions, tiers or levels resolve it;
- the startup sweep: the first resolve walks the image's IGNORE and REFUSE
  rows (the generated table of D5) and every `run:` retired spelling;
- a C ABI (`nros_env_get`, `nros_env_resolve_str`, `nros_env_resolve_u32`,
  `nros_env_notice_if_set`) and `<nros/env.h>`, which returns NULL under
  `__STDC_HOSTED__ == 0` as `env_compat.hpp` does today, so the Cyclone C++
  backend and the XRCE C transport call it.

A row is a `&'static EnvRow`: core rows from the committed table, a
package's rows from its own `$OUT_DIR/nros_knobs.rs` (D5). Warnings go
through `nros_log::log_warn!` on logger `nros.env` (RFC-0102), never a raw
stderr write (issue 0589); notices are `log_info!`; plain lines first, with
`NROS-NNNN` codes later under RFC-0129 (Decision 10). `nros::env` stays the
boot-config ladder and its `report`, built on `nros-env`.

### D12 - Stock ROS 2 semantics, and where nano-ros differs

Each becomes an RFC-0036 divergence row when it lands.

- **Discovery range.** Jazzy deprecates `ROS_LOCALHOST_ONLY` for
  `ROS_AUTOMATIC_DISCOVERY_RANGE` plus `ROS_STATIC_PEERS`; Humble has only
  the former. nano-ros IGNOREs all three with a notice (issue 1009 measured
  that a half-isolated pair finds nothing).
- **`RMW_IMPLEMENTATION`.** Stock loads an RMW library by it; a nano-ros
  image links its backends at build time and uses `NROS_RMW` among them.
- **`ROS_DOMAIN_ID` on embedded.** Stock always reads it; D10's note is the
  replacement.
- **Logging.** Stock rcutils reads four `RCUTILS_*` variables and writes
  files under `ROS_LOG_DIR`; nano-ros honours two of them on Linux and writes
  no files.
- **Security.** Stock enforces; nano-ros refuses the enforced case.
- **Precedence.** Stock has no statement rung; the warning is how a ROS user
  sees that one existed.

## Part C - Gates

### D13 - The gates

Gates are tests in `packages/testing/nros-gates` (RFC-0131). Each has a
self-test that plants the violation in a temporary tree and asserts the gate
names it.

- **G1 - `config-names` (the lint is the loader).** The catalog loader
  enforces N1 to N8 and C1 to C4, so an out-of-tree package with a bad name
  fails to load in the user's own build, not only in our CI. The gate runs
  the loader over every in-tree contributor. It lands with a baseline equal
  to Appendix A that may only shrink, and the rename stages empty it.
- **G2 - `knob-registry` (existing), widened.** The committed views equal
  the render of the merged in-tree set; `zephyr-kconfig-knobs` keeps its
  check; the generated `RETIRED_ENV_VARS` view joins it.
- **G3 - `config-reads-through-registry`** (the first draft's D5, widened
  to build reads). Every run-time read primitive (`getenv(`, `std::getenv`,
  `env::var(`, `env::var_os(`, `env_lookup(`) outside `nros-env`, and every
  build-script read (`rerun-if-env-changed=`, `env::var` in a build script,
  `option_env!`), names a registry row, a resource row (RFC-0103 D6), or a
  variable cargo or the toolchain defines. Hand-written core code reading a
  package-owned name fails (D2). The file set is derived from `git ls-files`,
  not authored (issue 0196's reach rule); a ratchet baseline lists today's
  bypasses.
- **G4 - `hand-kept-config-lists`.** Every `config NROS_*` outside a
  generated block in `zephyr/Kconfig` and `integrations/nuttx/Kconfig`, every
  typed tenant field, and every non-`INTERNAL` cmake `NROS_*` cache entry or
  option is a row or an entry of `config-exemptions.toml`, which carries a
  reason per entry. Reasons are free text, but an entry with an empty reason
  fails.
- **G5 - `retired-spellings-refused`** (replaces
  `check-retired-knob-spellings` and the alias half of
  `check-retired-runtime-aliases`). For every retired spelling, a generated
  case on each road its tag names asserts the refusal and that the message
  names the replacement.
- **G6 - the contributor fixtures.** The out-of-tree RMW fixture
  (`packages/testing/nros-tests/fixtures/out-of-tree-rmw/`, RFC-0071 D16)
  and the outside board fixture (phase-577 W8) each declare one knob and one
  `env` row. Each image builds with the knob stated, the knob's value reaches
  the package's build script, `nros knob show acme.<name>` prints it, and the
  env row is reported by the startup sweep. A third fixture declares an owner
  word already taken and asserts C1's message.

## Staging

Phase-581 stages the work so that every rename is its own stage, and so the
registry can carry contributors before any name moves:

1. Measure (W0): the first draft's run-time measurements, plus M5 (an
   `osource`d choice member), the undefined-symbol behaviour of Zephyr's
   Kconfig, the demand-carrier census, and the owners of the rows N3 cannot
   decide by reading.
2. The registry gains kinds, `key`, `owner` and `retired`, with no behaviour
   change (W1); then contributors, with the in-tree packages' rows moved out
   (W2); then out-of-tree contributors and their per-build views (W3).
3. The naming lint lands with Appendix A as its baseline (W4).
4. Renames, one stage per class (W5 to W7), each emptying part of the
   baseline and adding its refusals.
5. The open RMW set (W8) and the hand-kept folds (W9).
6. Part B's reader and its batches (W10 to W15), which may run beside 3 to 5
   because they depend only on W1.

## Alternatives considered

- **A sibling `env-vars.toml` registry.** Rejected by the user's ruling: two
  registries can each be consistent and still disagree with each other.
- **A phase prefix (`NROS_BUILD_*` for carriers, `NROS_*` for run-time).**
  Rejected: four names collide across phases today, and a prefix would rename
  all 112 carriers to fix them, although carriers are not user surface
  (RFC-0133). N5 plus the load-time uniqueness check fixes the four.
- **Allow TOML sub-tables, with every segment in every spelling.**
  Rejected: `CONFIG_NROS_ZENOH_WIRE_FRAG_MAX_SIZE` makes a display grouping
  permanent in every spelling, and ownership needs one segment to check.
- **Keep D2b's Kconfig column as a permanent alias.** Rejected by ruling 2,
  and by measurement: the column is how two facts (demand and pool) came to
  share one symbol.
- **Package rows by pull request to the core file.** Rejected: that is
  RFC-0049's out-of-tree wall, and a downstream vendor cannot wait for our
  merge queue.
- **Compile package rows into core's `KNOBS` per image** (the build helper
  reads the catalog at its own build time). Rejected: the helper's
  fingerprint then depends on the image's package set, so two images sharing
  a target directory rebuild each other (the issue 0491 class). A file read
  at build-script run time has no such edge.
- **A descriptor field naming the rows file.** Rejected: a second place
  naming one file. The file beside the descriptor is found by the scan that
  already finds the descriptor.
- **Open deep-merge of package files** (cargo or Nix style). Not what this
  is: packages contribute DECLARATIONS under disjoint owner words, refused on
  overlap. VALUES keep RFC-0004's and RFC-0049's fixed ladder.
- **Deprecate first, refuse a release later.** Rejected by ruling 2. A
  rename lands with its refusal.

## Non-goals

- Changing any default or any value; a rename keeps the value (a unit change
  keeps the quantity and states the factor).
- Command-line `--ros-args`: argv, owned by `nros::init_with_args`.
- Resource locations (`NROS_HOME`, `FREERTOS_DIR`, `NROS_MODEL_DIR`,
  `NROS_LAUNCH_RESOLVE`, ...): RFC-0103 D6's index rows. N8 only keeps the
  two namespaces apart.
- A remote package index (RFC-0135 D5 leaves it out too).
- A board package's own `[arch.*]` profiles: RFC-0135 D4.
- Re-deciding which pools are derived; the demand split only names the two
  facts the code already has.
- C macro names inside a backend (`ZPICO_*`, `XRCE_*` defines,
  `NROS_RMW_UORB_PX4_MAX_CALLBACKS`): generated, never read from an
  environment (N2).

## Open questions

None for the user. The items that depended on a measurement (M5, M6,
`ROS_NAMESPACE`, `native_sim`, the demand-carrier census, the owner of
`SERVICE_TIMEOUT_MS`, the units of seven rows whose help does not state one,
the cmake entries) were measured in W0 ([data](../roadmap/data/phase-581-w0-measure.md)) and the text above
carries the results. Not measured: `ROS_NAMESPACE` on Jazzy, and the
`freertos-posix` Rust road at run time (Decision 11 rests on reading).

## Decisions (user ruling, 2026-11-19)

The user accepted the first draft's recommendations on Q2 to Q11 and
reopened Q1 with a wider scope; this revision answers Q1.

1. **Registry home: one registry with the knobs** (Q1, reopened, answered by
   D1 to D7 and D13). Environment variables are `env` rows of the knob
   registry under one naming scheme; board and RMW packages declare their own
   rows; rule-breaking names are renamed now and their old spellings refused.
2. **Discovery range: IGNORE with a notice** for `ROS_LOCALHOST_ONLY`,
   `ROS_AUTOMATIC_DISCOVERY_RANGE` and `ROS_STATIC_PEERS`. RESPECT on the
   Cyclone backend is later work.
3. **`ROS_NAMESPACE`: match stock rclcpp**, after measuring what rcl and
   rclcpp do with it; the `ExecutorConfig::namespace` doc follows the
   measurement. *Measured (W0):* on Humble, `ROS_NAMESPACE=/w0ns` left
   `demo_nodes_cpp` and `demo_nodes_py` talkers at `/talker` and `/chatter`
   (`--ros-args -r __ns:=/w0ns` gave `/w0ns/talker`), and no rcl, rclcpp,
   rcutils or rclpy file names it. So nano-ros IGNOREs it with a notice
   naming `NROS_NODE_NAMESPACE`. Jazzy: not measured (no Jazzy install or
   image on the measuring host).
4. **nano-ros' own `ZENOH_*` variables: RETIRE.** The six inventions are
   REFUSED, each naming its replacement, the stock `ZENOH_CONFIG_OVERRIDE`,
   lowered by the same reader as the session file.
5. **Logging: RESPECT** `RCUTILS_COLORIZED_OUTPUT` and
   `RCUTILS_CONSOLE_OUTPUT_FORMAT` in the Linux console sink. `RUST_LOG`
   stays Rust-only.
6. **Loans: IGNORE** `ROS_DISABLE_LOANED_MESSAGES`.
7. **Router variables:** `ZENOH_ROUTER_CONFIG_URI` gets a notice like every
   other ignored row.
8. **`native_sim`: measure** whether its libc sees the host environment,
   then correct whichever of RFC-0109 D6a and the Cyclone backend's comment
   is wrong. *Measured (W0):* with `CONFIG_PICOLIBC=y`, which every nano-ros
   `native_sim` image uses (Zephyr's POSIX options select
   `NATIVE_LIBC_INCOMPATIBLE`), `getenv` returns NULL for every name on 3.7
   and 4.4; with `CONFIG_EXTERNAL_LIBC=y` it sees the host environment.
   RFC-0109 D6a, which lists `native_sim` among hosted builds the environment
   overrides, is wrong (corrected in W16); the Cyclone backend's comment is
   right.
9. **NuttX: RESPECT.** The reader is compiled for `target_os = "nuttx"`.
10. **Diagnostic codes: plain log lines first.** `NROS-NNNN` codes come
    later, under RFC-0129.
11. **`freertos-posix` Rust: RESPECT**, as `threadx-linux` does through
    `resolve_boot`, so its Rust road agrees with its C/C++ road. *W0:* by
    reading, the Rust road resolves through `ExecutorConfig::resolve`, which
    has no environment rung (`nros-board-freertos/src/entry.rs:376`). Not
    measured at run time: no in-tree Rust `freertos-posix` image exists, and a
    scratch one did not build (its node crate pulls `std` through
    `env_logger`).

## Related

- RFC-0103 (D2b and D6 amended here), RFC-0049 (one file per package; its
  "schema declared centrally" becomes "declared per owner, merged at load"),
  RFC-0071 D16 and D18 (the out-of-tree RMW fixture; D18's closed set
  opened), RFC-0133 (the knob catalog's source becomes the merged catalog),
  RFC-0135 D5 (where a board package is found), RFC-0114 (a downstream
  package's registry row: the same merge shape), RFC-0004 (fixed ladder),
  RFC-0098 D10/D11 (the image rung), RFC-0109 D6a and D8, RFC-0045,
  RFC-0036, RFC-0102, RFC-0129, RFC-0131, RFC-0072 (the Zephyr guest),
  RFC-0065 (the colcon-like builder that builds these packages).
- Issue 2714 and PR #4285; issues 0196, 0491, 0687, 0794, 1009, 1015, 1033,
  2194; phase-484 W8 (the retired-spelling pattern), phase-569 W2 (the
  catalog), phase-577 W8 (the outside board fixture).

## Changelog

- 2026-11-19: Draft (ROS 2 environment variables against `system.toml`).
- 2026-11-19: Q2 to Q11 decided by the user; Q1 reopened.
- 2026-11-19: **Widened** to one configuration registry, per the user's
  ruling: three row kinds in one namespace (D1); contributors with owner
  words and load-time collision rules (D2); a complete naming rule amending
  RFC-0103 D2b and D6 (D3); retired spellings on the replacing row, refused
  on every road (D4); per-build views for out-of-tree rows (D5);
  self-contained packages and an open Zephyr RMW set, amending RFC-0071 D18
  (D6); hand-kept lists folded (D7); the environment-variable design
  re-homed as Part B (D8 to D12); gates widened to build reads and the
  naming rule (D13); the 86-name rename list (Appendix A). Title changed;
  `amends` lists RFC-0103, RFC-0049, RFC-0071, RFC-0133.
- 2026-11-19: **Accepted by the user.** File renamed from
  `0140-ros-env-vars.md` to `0140-configuration-registry.md`. Appendix A
  gains `NROS_THREADX_BYTE_POOL_KB` (PR #4355, merged after the
  measurement), renamed to `threadx.byte_pool_bytes` in W5; 87 entries.
- 2026-11-19: **phase-581 W0 measured** (data in
  `docs/roadmap/data/phase-581-w0-measure.md`). M5 green on 3.7 and 4.4
  (D6: no `NROS_RMW_EXTERNAL` fallback; the hook refuses a conflicting conf
  member, since Kconfig takes the last silently); M6 (D4): an undefined
  symbol fails configure on both lines; `SERVICE_TIMEOUT_MS` is core
  (`service.timeout_ms`); `LET_BUFFER_SIZE` has no reader and is retired, so
  A.1 is 52 renamed and 2 retired (87 unchanged); the `NROS_MAX_*` census
  (D3, cmake carriers are Zephyr-only); 52 user-settable cmake entries
  classified (D7); Appendix B gains a measured column; `native_sim` has no
  environment (Decision 8, D10); `ROS_NAMESPACE` IGNORE (Decision 3, Humble);
  `NROS_NODE_NAME`/`NROS_NODE_NAMESPACE` are read but do not rename a
  component or application node (D9).

## Appendix A - Settings that break the rule on `origin/main` (87)

Columns: the old TOML key (a registry row with no `toml` column shows the key
the image rung uses, its id split at the first `_`), the old build env, the
old Kconfig symbol where it differs from `CONFIG_` plus the env, and the new
key. Every new spelling is mechanical from the new key (N2). Reasons: **S**
three segments (N1), **U** unit (N4), **K** Kconfig column (N2), **P** owner
word missing or wrong (N3), **F** folds two platform carriers into one
`memory.*` knob.

### A.1 Registry rows (52 renamed, 2 retired)

| Old key | Old env | Old Kconfig | New key | Why |
| --- | --- | --- | --- | --- |
| `executor.arena_size` | `NROS_EXECUTOR_ARENA_SIZE` |  | `executor.arena_bytes` | U |
| `executor.backing_u64s` | `NROS_EXECUTOR_BACKING_U64S` |  | `executor.backing_bytes` | U |
| `freertos.app_stack_kb` | `NROS_FREERTOS_APP_STACK_KB` |  | `memory.app_stack_bytes` | U, F |
| `freertos.heap_kb` | `NROS_FREERTOS_HEAP_KB` |  | `memory.heap_bytes` | U, F |
| `graph.max_entities` | `NROS_GRAPH_MAX_ENTITIES` |  | `zenoh.graph_max_entities` | P |
| `zenoh.limits.keyexpr_string_size` | `NROS_KEYEXPR_STRING_SIZE` |  | `zenoh.keyexpr_string_bytes` | S, U |
| `runtime.let_buffer_size` | `NROS_LET_BUFFER_SIZE` |  | retired: no reader (W0) | U, P |
| `log.buffer_size` | `NROS_LOG_BUFFER_SIZE` |  | `log.buffer_bytes` | U |
| `params.max_array_len` | `NROS_MAX_ARRAY_LEN` |  | `params.max_array_len` | P |
| `params.max_byte_array_len` | `NROS_MAX_BYTE_ARRAY_LEN` |  | `params.max_byte_array_bytes` | U, P |
| `params.max_parameters` | `NROS_MAX_PARAMETERS` |  | `params.max_parameters` | P |
| `params.max_param_constraints_len` | `NROS_MAX_PARAM_CONSTRAINTS_LEN` |  | `params.max_constraints_bytes` | U, P |
| `params.max_param_description_len` | `NROS_MAX_PARAM_DESCRIPTION_LEN` |  | `params.max_description_bytes` | U, P |
| `params.max_param_name_len` | `NROS_MAX_PARAM_NAME_LEN` |  | `params.max_name_bytes` | U, P |
| `params.max_string_array_len` | `NROS_MAX_STRING_ARRAY_LEN` |  | `params.max_string_array_len` | P |
| `params.max_string_value_len` | `NROS_MAX_STRING_VALUE_LEN` |  | `params.max_string_value_bytes` | U, P |
| `params.param_events_depth` | `NROS_PARAM_EVENTS_DEPTH` |  | `params.events_depth` | P |
| `executor.param_service_buffer_size` | `NROS_PARAM_SERVICE_BUFFER_SIZE` |  | `executor.param_service_buffer_bytes` | U, P |
| `param.service_inbox_bytes` | `NROS_PARAM_SERVICE_INBOX_BYTES` |  | `params.service_inbox_bytes` | P |
| `param.service_inbox_depth` | `NROS_PARAM_SERVICE_INBOX_DEPTH` |  | `params.service_inbox_depth` | P |
| `zenoh.limits.service_timeout_ms` | `NROS_SERVICE_TIMEOUT_MS` |  | `service.timeout_ms` | S, P |
| `net.buffer_size` | `NROS_SMOLTCP_BUFFER_SIZE` |  | `net.buffer_bytes` | U, P |
| `net.connect_timeout_ms` | `NROS_SMOLTCP_CONNECT_TIMEOUT_MS` |  | `net.connect_timeout_ms` | P |
| `net.max_sockets` | `NROS_SMOLTCP_MAX_SOCKETS` |  | `net.max_sockets` | P |
| `net.max_udp_sockets` | `NROS_SMOLTCP_MAX_UDP_SOCKETS` |  | `net.max_udp_sockets` | P |
| `net.socket_timeout_ms` | `NROS_SMOLTCP_SOCKET_TIMEOUT_MS` |  | `net.socket_timeout_ms` | P |
| `subscriber.buffer_size` | `NROS_SUBSCRIBER_BUFFER_SIZE` |  | `subscriber.buffer_bytes` | U |
| `executor.subscription_buffer_size` | `NROS_SUBSCRIPTION_BUFFER_SIZE` |  | `executor.subscription_buffer_bytes` | U, P |
| `threadx.app_stack_kb` | `NROS_THREADX_APP_STACK_KB` |  | `memory.app_stack_bytes` | U, F |
| (carrier of `memory.heap_bytes`) | `NROS_THREADX_BYTE_POOL_KB` |  | `threadx.byte_pool_bytes` | U |
| `xrce.buffer_size` | `NROS_XRCE_BUFFER_SIZE` |  | `xrce.buffer_bytes` | U |
| `xrce.builtin_service_request_buffer_size` | `NROS_XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE` |  | `xrce.builtin_service_request_buffer_bytes` | U |
| `xrce.service_reply_buffer_size` | `NROS_XRCE_SERVICE_REPLY_BUFFER_SIZE` |  | `xrce.service_reply_buffer_bytes` | U |
| `xrce.service_request_buffer_size` | `NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE` |  | `xrce.service_request_buffer_bytes` | U |
| `xrce.subscriber_buffer_size` | `NROS_XRCE_SUBSCRIBER_BUFFER_SIZE` |  | `xrce.subscriber_buffer_bytes` | U |
| `zenoh.wire.batch_multicast_size` | `NROS_ZENOH_BATCH_MULTICAST_SIZE` |  | `zenoh.batch_multicast_bytes` | S, U |
| `zenoh.wire.batch_unicast_size` | `NROS_ZENOH_BATCH_UNICAST_SIZE` | `CONFIG_NROS_BATCH_UNICAST_SIZE` | `zenoh.batch_unicast_bytes` | S, U, K |
| `zenoh.limits.connect_forever` | `NROS_ZENOH_CONNECT_FOREVER` |  | `zenoh.connect_forever` | S |
| `zenoh.limits.connect_timeout_ms` | `NROS_ZENOH_CONNECT_TIMEOUT_MS` |  | `zenoh.connect_timeout_ms` | S |
| `zenoh.wire.frag_max_size` | `NROS_ZENOH_FRAG_MAX_SIZE` | `CONFIG_NROS_FRAG_MAX_SIZE` | `zenoh.frag_max_bytes` | S, U, K |
| `zenoh.wire.get_poll_interval_ms` | `NROS_ZENOH_GET_POLL_INTERVAL_MS` | `CONFIG_NROS_GET_POLL_INTERVAL_MS` | `zenoh.get_poll_interval_ms` | S, K |
| `zenoh.wire.get_reply_buf_size` | `NROS_ZENOH_GET_REPLY_BUF_SIZE` | `CONFIG_NROS_GET_REPLY_BUF_SIZE` | `zenoh.get_reply_buffer_bytes` | S, U, K |
| `zenoh.graph_cache_size` | `NROS_ZENOH_GRAPH_CACHE_SIZE` | `CONFIG_NROS_GRAPH_CACHE_SIZE` | `zenoh.graph_cache_bytes` | U, K |
| `zenoh.publisher_tx_buffer_size` | `NROS_ZENOH_PUBLISHER_TX_BUFFER_SIZE` |  | `zenoh.publisher_tx_buffer_bytes` | U |
| `zenoh.wire.rx_task_budget_frames` | `NROS_ZENOH_RX_TASK_BUDGET_FRAMES` |  | `zenoh.rx_task_budget_frames` | S |
| `zenoh.wire.rx_task_budget_rest_us` | `NROS_ZENOH_RX_TASK_BUDGET_REST_US` |  | `zenoh.rx_task_budget_rest_us` | S |
| `zenoh.subscriber_large_size` | `NROS_ZENOH_SUBSCRIBER_LARGE_SIZE` | `CONFIG_NROS_SUBSCRIBER_LARGE_SIZE` | `zenoh.subscriber_large_bytes` | U, K |
| `zenoh.limits.subscriber_ring_depth` | `NROS_ZENOH_SUBSCRIBER_RING_DEPTH` | `CONFIG_NROS_SUBSCRIBER_RING_DEPTH` | `zenoh.subscriber_ring_depth` | S, K |
| `zenoh.subscriber_size_threshold` | `NROS_ZENOH_SUBSCRIBER_SIZE_THRESHOLD` |  | `zenoh.subscriber_large_threshold_bytes` | U |
| `zenoh.tx.batch` | `NROS_ZENOH_TX_BATCH` |  | `zenoh.tx_batch` | S |
| `zenoh.tx.flush_ms` | `NROS_ZENOH_TX_BATCH_FLUSH_MS` |  | `zenoh.tx_batch_flush_ms` | S |
| `zenoh.tx.split_lock` | `NROS_ZENOH_TX_SPLIT_LOCK` |  | `zenoh.tx_split_lock` | S |
| `zephyr.heap_size` | `NROS_ZEPHYR_HEAP_SIZE` |  | `memory.heap_bytes` | U, F |

Also in the registry:

- `zenoh.service_buffer_size` (`NROS_ZENOH_SERVICE_BUFFER_SIZE`,
  `CONFIG_NROS_SERVICE_BUFFER_SIZE`): its own help calls it "the one-release
  ALIAS of NROS_SERVICE_INBOX_BYTES". RETIRED, refused naming
  `service.inbox_bytes`.
- `entry.spin_ms` (`NROS_ENTRY_SPIN_MS`): no build-time reader (W0
  confirmed: no `rerun-if-env-changed`, `option_env!` or `CONFIG_` reader);
  its 7 readers are run-time. It moves from a `knob` row to an `env` row of
  the same name. Not a rename.
- `runtime.let_buffer_size` (`NROS_LET_BUFFER_SIZE`): W0 found that the
  constant it bakes into `nros-c` (`LET_BUFFER_SIZE`) has no reader, so the
  row is retired with no replacement (class `retired`, `instead` saying it
  had no effect) rather than renamed.
- `executor.backing_u64s` to `executor.backing_bytes` changes the unit: the
  refusal states "x 8". `freertos.heap_kb`, `freertos.app_stack_kb`,
  `threadx.app_stack_kb` and `NROS_THREADX_BYTE_POOL_KB` state "x 1024".
- `NROS_THREADX_BYTE_POOL_KB` (issue 2624, PR #4355, added after the
  measurement at `365ad8d9e6`): the ThreadX byte pool is the port's whole
  allocator, not the heap alone, so it becomes its own ThreadX row,
  `threadx.byte_pool_bytes`, rather than folding into `memory.heap_bytes`;
  its derived default (app stack plus the RMW's working set) is unchanged.
- Units of seven rows whose help states none: W0 read each consumer
  ([data](../roadmap/data/phase-581-w0-measure.md), section 8). `KEYEXPR_STRING_SIZE`, `SMOLTCP_BUFFER_SIZE`,
  `ZENOH_BATCH_MULTICAST_SIZE`, `ZENOH_PUBLISHER_TX_BUFFER_SIZE`,
  `ZENOH_SUBSCRIBER_SIZE_THRESHOLD` and
  `XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE` are bytes (a byte array, a
  string capacity or a comparison with payload lengths); `LET_BUFFER_SIZE`
  has no reader (above).

### A.2 Hand-written `zephyr/Kconfig` symbols (12)

| Old symbol | New symbol | Why |
| --- | --- | --- |
| `CONFIG_NROS_CYCLONE_DOMAIN_ID` | `CONFIG_NROS_CYCLONEDDS_DOMAIN_ID` | P (`cmake/NanoRosDomainAgreement.cmake` keeps it equal to the domain) |
| `CONFIG_NROS_CYCLONE_CONFIG_XML` | `CONFIG_NROS_CYCLONEDDS_CONFIG_XML` | P |
| `CONFIG_NROS_EPOCH_FROM_ZENOH` | `CONFIG_NROS_ZENOH_WALL_CLOCK_EPOCH` | P (depends on `NROS_RMW_ZENOH`) |
| `CONFIG_NROS_EPOCH_FROM_ZENOH_INTERVAL_MS` | `CONFIG_NROS_ZENOH_WALL_CLOCK_EPOCH_INTERVAL_MS` | P |
| `CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE` | `CONFIG_NROS_ZEPHYR_TASK_STACK_BYTES` | U |
| `CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE` | `CONFIG_NROS_ZEPHYR_TIER_STACK_BYTES` | U |
| `CONFIG_NROS_ZEPHYR_SCHED_TASK_STACK_SIZE` | `CONFIG_NROS_ZEPHYR_SCHED_TASK_STACK_BYTES` | U |
| `CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME` | `CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME_BYTES` | U (prompt says bytes) |
| `CONFIG_NROS_CONSOLE_WHOLE_LINES_BUFFER` | `CONFIG_NROS_CONSOLE_WHOLE_LINES_BUFFER_BYTES` | U (prompt says bytes) |
| `CONFIG_NROS_EDMA_WARM_RESET_QUIESCE` | `CONFIG_NROS_ZEPHYR_EDMA_WARM_RESET_QUIESCE` | P (a Zephyr SoC-driver workaround read by `zephyr/CMakeLists.txt`) |
| `CONFIG_NROS_PARAM_STORE` | `CONFIG_NROS_PARAMS_STORE` | P (`params`, not `param`) |
| `CONFIG_NROS_TRANSPORT_SERIAL` | retired: `[image.<id>] transport = "serial"` in a managed project, `CONFIG_NROS_ZENOH_LINK_SERIAL` in a guest | it only `select`s the zenoh serial link; transport is an image statement (RFC-0098) |

### A.3 Run-time variables (3 renamed, 6 retired)

| Old | New | Why |
| --- | --- | --- |
| `XRCE_SERIAL_BAUD` | `NROS_XRCE_SERIAL_BAUD` | N6 (ours, in no `NROS_` prefix) |
| `NROS_CYCLONE_MATCH_TIMEOUT_MS` | `NROS_CYCLONEDDS_MATCH_TIMEOUT_MS` | P |
| `NROS_RMW_CYCLONEDDS_SKIP_PREFIX` | `NROS_CYCLONEDDS_SKIP_PREFIX` | P |
| `ZENOH_MULTICAST_SCOUTING`, `ZENOH_SCOUTING_TIMEOUT`, `ZENOH_LISTEN`, `ZENOH_TLS_ROOT_CA_CERTIFICATE`, `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64`, `ZENOH_TLS_VERIFY_NAME_ON_CONNECT` | retired: `ZENOH_CONFIG_OVERRIDE` | N6, Decision 4 |

### A.4 Build carriers outside the registry (6)

| Old | New | Why |
| --- | --- | --- |
| `ZPICO_NO_SMOLTCP` | `NROS_ZENOH_NO_SMOLTCP` (a carrier row, written by `nros` from `transport = "serial"`) | N6, P |
| `ZPICO_MULTICAST_TRANSPORT` | `zenoh.multicast_transport` (a public knob: the zenoh backend's own error text tells users to set it) | N6, P |
| `NROS_ZPICO_DEBUG` | `NROS_ZENOH_DEBUG` | P |
| `NROS_DOMAIN_ID` (build) | `NROS_ENTRY_DOMAIN_ID` (refused at build, naming `[system] domain_id`) | N5; RFC-0103 section 3 already lists it as a retired input |
| `NROS_LOCATOR` (build, `option_env!` in `Context::baked` and the `mps2-an385` and `nuttx-qemu` board configs) | `NROS_ENTRY_LOCATOR`; `NROS_LOCATOR` stays the run-time variable | N5 |
| `NROS_NODE_NAME` (build, `nros-board-mps2-an385/src/rtic.rs`) | `NROS_ENTRY_NODE_NAME`; `NROS_NODE_NAME` stays the run-time variable | N5 |

### A.5 The demand symbols (6; staged after a census)

| Old Kconfig / carrier | New |
| --- | --- |
| `CONFIG_NROS_MAX_PUBLISHERS` / `NROS_MAX_PUBLISHERS` | `entity.max_publishers` |
| `CONFIG_NROS_MAX_SUBSCRIBERS` / `NROS_MAX_SUBSCRIBERS` | `entity.max_subscribers` |
| `CONFIG_NROS_MAX_QUERYABLES` / `NROS_MAX_QUERYABLES` | `entity.max_queryables` |
| `CONFIG_NROS_MAX_LIVELINESS` / `NROS_MAX_LIVELINESS` | `entity.max_liveliness` |
| `CONFIG_NROS_MAX_PENDING_GETS` / `NROS_MAX_PENDING_GETS` | `entity.max_pending_gets` |
| `CONFIG_NROS_MAX_LARGE_SUBSCRIBERS` / `NROS_MAX_LARGE_SUBSCRIBERS` | `entity.max_large_subscribers` |

The zenoh pool rows keep their keys (`zenoh.max_publishers`, ...) as
`class = "carrier"` rows with no Kconfig symbol.

## Appendix B - Run-time inventory (first draft, measured 2026-11-19)

**Method.** `git grep` for each variable over
`packages/{api,core,rmw,platform,boards,drivers}`, `cmake/` and `zephyr/`,
excluding tests, third-party, generated code and Markdown, then every
run-time read primitive (`getenv(`, `std::getenv`, `env::var(`,
`env::var_os(`, `nros::env::var(`, `env_lookup(`, `option_env!(`), each hit
read to classify it. Platform reach was read from the `cfg` guarding each
reader, not run. **W0 ran it** ([data](../roadmap/data/phase-581-w0-measure.md), section 1): a Rust, a C and a C++
`linux` zenoh image with every variable set, under an `LD_PRELOAD` shim that
logs each `getenv`, plus a `native_sim` probe on 3.7 and 4.4. The last column
of each table is that run: it names the roads (Rust, C, C++) whose image
read the variable; "-" means no image read it on its boot path. Re-checked on `origin/main` `365ad8d9e6`, which
adds three run-time names the first draft missed:
`NROS_ENTRY_SPIN_STEP_MS` and `NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS` (the
`nros::main!` expansion) and confirms `NROS_ZPICO_DEBUG` is build-time
(`nros-zpico-build`).

"Rust" is `nros::main!` on a Rust board; "C/C++" is the `nros-c`/`nros-cpp`
staticlib. "Linux" means a Linux process: the `linux` board, `threadx-linux`
and `freertos-posix`. Zephyr `native_sim` is NOT one for this purpose (W0: its
picolibc `getenv` sees no host variable), although the first draft counted it.

### ROS 2 standard variables

| Variable | Read today? Where | Platforms and roads | What `system.toml` states for the same fact | W0 measured (2026-11-19) |
| --- | --- | --- | --- | --- |
| `ROS_DOMAIN_ID` | Yes. `nros::env::env_rung` (parsed; malformed is `BootConfigError::DomainIdParse`, over 232 is `DomainIdRange`) | Every Linux process, C/C++ road (`resolve_boot`). Rust road: `linux` board (`resolve_hosted`), `threadx-linux` (`nros-board-threadx::entry::resolve_boot`). **Not** the Rust road on `freertos-posix`: `nros-board-freertos::entry` calls `ExecutorConfig::resolve`, no env rung. Never on embedded | `[system] domain_id` (required), `[image.<id>] domain_id`. Baked as `DeployOverlay.domain_id` (Rust), `NROS_ENTRY_DOMAIN_ID` (C), `CONFIG_NROS_DOMAIN_ID` (Zephyr) | Rust, C, C++ read; acted (boot `domain 7 (env)`; Rust warned the override; 999 refused on all three) |
| `ROS_LOCALHOST_ONLY` | Not read | none | Nothing. The middleware file (`rmw/cyclonedds.xml`, `rmw/zenoh.json5`) can state the same restriction | - on all three (confirmed) |
| `ROS_AUTOMATIC_DISCOVERY_RANGE` | Not read | none | Nothing; same as above | - on all three (confirmed) |
| `ROS_STATIC_PEERS` | Not read | none | Nothing; the middleware file's peer list (Cyclone `<Peers>`, zenoh `connect/endpoints`) | - on all three (confirmed) |
| `RMW_IMPLEMENTATION` | One reader: `nros::init` (`init.rs`, `Context::default_from_env` path) as a fallback for the `Context.rmw` HINT only; no measured image reaches it. Deliberately NOT a backend selector (`nros::env::rmw_selector` doc) | Rust road, `env` feature, Linux. C/C++: not read | `[system] rmw`, `[image.<id>] rmw` (a nano-ros name: `zenoh`, `cyclonedds`, `xrce`) | - on all three: its one reader is not on the `linux` board's boot path (corrects "Rust road") |
| `ROS_NAMESPACE` | Not read. Deliberately (`nros_node` `ExecutorConfig::namespace` doc) | none | The launch file's namespace, baked in the boot blob | - on all three (confirmed); stock Humble ignores it too (Decision 3) |
| `ROS_LOG_DIR`, `ROS_HOME` | Not read (the `ROS_HOME` hits are `NROS_HOME`, a CLI store path) | none | Nothing. nano-ros writes no log files | - on all three (confirmed) |
| `RCUTILS_CONSOLE_OUTPUT_FORMAT`, `RCUTILS_COLORIZED_OUTPUT`, `RCUTILS_LOGGING_USE_STDOUT`, `RCUTILS_LOGGING_BUFFERED_STREAM` | Not read (`RCUTILS_*` appears only in `nros-log` doc comments naming upstream macros) | none | Nothing | - on all three (confirmed) |
| `ROS_DISABLE_LOANED_MESSAGES` | Not read. A loan primitive exists (`borrow_loaned_message` slot, `nros/rmw_entity.h`) | none | Nothing | - on all three (confirmed) |
| `ROS_SECURITY_ENABLE`, `ROS_SECURITY_STRATEGY`, `ROS_SECURITY_KEYSTORE`, `ROS_SECURITY_ENCLAVE_OVERRIDE` | Not read | none | Nothing; nano-ros has no SROS2 support | - on all three, `Enforce` included (confirmed) |
| `RUST_LOG` (not ROS, but the one log-level variable read) | Yes: `nros-board-linux` stdout sink (`rust_log_default_level`), only when that sink is installed | Rust road, `linux` board only. C/C++ and other boards: not read | Nothing | Rust only; acted (`debug` printed `[DEBUG]` lines) |

### Backend variables

| Variable | Read today? Where | Platforms and roads | `system.toml` equivalent | W0 measured (2026-11-19) |
| --- | --- | --- | --- | --- |
| `ZENOH_SESSION_CONFIG_URI` | Yes: `nros-rmw-zenoh` `session_config_uri_properties`, through `getenv`, lowered by `nros-zenoh-session-config::lower`; replaces the baked table whole (RFC-0109 D8 amendment, rule 2) | Linux, both roads, when the backend has `session-config-uri` (Rust `std`; C/C++ through the descriptor's `alloc` feature, `nros-rmw.toml`) | `rmw_config` (default `rmw/zenoh.json5`), lowered at build | Rust, C, C++ read; acted on all three (with `NROS_LOCATOR` unset the file's `connect` opened the session; with it set, `NROS_LOCATOR` won) |
| `ZENOH_MULTICAST_SCOUTING`, `ZENOH_SCOUTING_TIMEOUT`, `ZENOH_LISTEN`, `ZENOH_TLS_ROOT_CA_CERTIFICATE`, `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64`, `ZENOH_TLS_VERIFY_NAME_ON_CONNECT` | Yes: `nros-rmw-zenoh` `ZenohSession::new` (property defaults) and `append_tls_env_to_locator`, through `std::env` | **Rust road only**: both readers are `#[cfg(feature = "std")]`; the `linux` board enables `nros-rmw-zenoh/std`, a C/C++ image crate names `platform-posix` + `alloc`, not `std`. So a C/C++ Linux process ignores them silently | The same keys in `rmw/zenoh.json5` | Rust only (confirmed; C and C++ -). Rust acted: a TLS variable failed a `tcp/` session open (`needs Z_FEATURE_LINK_TLS`), `ZENOH_LISTEN` failed it too |
| `ZENOH_CONFIG_OVERRIDE` | Not read by an image. `nros_zenoh_tools` and `nros-orchestration-ir::router_rule` PRODUCE a value for `rmw_zenohd` | none | Nothing | - on all three (confirmed) |
| `ZENOH_ROUTER_CONFIG_URI` | Not read (a router variable; nano-ros ships no router) | none | Nothing | - on all three (confirmed) |
| `RMW_ZENOH_*` | Not read. The hits are `rmw_zenoh_cpp`'s QoS default constants named in comments | none | Nothing | - (`RMW_ZENOH_CONFIG_URI` set: not read) |
| `ZENOH_LOCATOR`, `ZENOH_MODE` | Refused: `nros::retired_env::RETIRED_ENV_VARS`, checked first in `env_rung`, naming `NROS_LOCATOR` / `NROS_SESSION_MODE` | Linux, every road through `env_rung` | (retired aliases) | Rust, C, C++ read; refused on all three (Rust panics, C/C++ init returns -3) |
| `CYCLONEDDS_URI` | Yes, two readers. Hosted POSIX: Cyclone's own config loader (third-party). The embedded compose path: `env_lookup` in `session.cpp`, which returns NULL on a freestanding build (`__STDC_HOSTED__ == 0`) | Linux, C/C++ and Rust (the Cyclone backend is one C++ library). On `native_sim` the comment in `session.cpp` says picolibc's `getenv` sees no host environment; W0 measured it so | `rmw_config` (default `rmw/cyclonedds.xml`), baked; Cyclone composes the two (a scalar in the env wins, a list element appends: RFC-0109 D8 amendment 3) | - on the zenoh images; Cyclone images not measured. `native_sim`: measured, no host environment (the comment is right) |

### nano-ros spellings

| Variable | Read today? Where | Platforms and roads | `system.toml` equivalent | W0 measured (2026-11-19) |
| --- | --- | --- | --- | --- |
| `NROS_LOCATOR` (run time) | Yes: `nros::env::env_rung`. A SET empty value is the env's answer (issue 0330) | As `ROS_DOMAIN_ID` | `[system]`/`[image.<id>] locator` | Rust, C, C++ read; acted (`tcp/127.0.0.1:1` failed to connect on all three) |
| `NROS_LOCATOR`, `NROS_DOMAIN_ID` (BUILD time) | Yes, as `option_env!`: `nros::init::Context::baked`, and the board configs of `freertos`, `mps2-an385`, `nuttx-qemu`, `threadx-linux`, `threadx-qemu-riscv64`; on Zephyr exported from `CONFIG_*` by `nros_zephyr_build::bake_nros_config` | Rust road, embedded and RTOS-on-host boards. A variable of the BUILD's shell, not the process's | Same keys; RFC-0103 D2 makes an exported variable an override at configure | `NROS_DOMAIN_ID` set at run time: - on all three (build-time only, confirmed). Build reads not run |
| `NROS_SESSION_MODE` | Yes: `env_rung`; anything but `peer` is `Client` | As `ROS_DOMAIN_ID` | Only the zenoh file's `mode` (RFC-0109 D8 amendment 1); nothing bakes a mode (`init.rs` `Context::baked` doc) | Rust, C, C++ read; `client` is the default, so no change observable |
| `NROS_RMW` | Yes: `nros::env::rmw_selector` and `env_rung` (one reader, issue 0687); `nros-c/src/executor.rs` reuses `rmw_selector` | As `ROS_DOMAIN_ID` | `rmw`; baked as the boot blob's `rmw` when the image links several backends | Rust, C, C++ read; acted (`rmw zenoh (env)`) |
| `NROS_NODE_NAME`, `NROS_NODE_NAMESPACE` | Yes: `env_rung`; empty means unset (issue 0794) | As `ROS_DOMAIN_ID` | The launch file's node name and namespace, baked | Rust, C, C++ read, but NOT acted on: the component (Rust) or application (C, C++) node kept its own name (`talker`) |
| `NROS_BRIDGE_<N>_LOCATOR`, `NROS_BRIDGE_<N>_DOMAIN` | Yes: `nros-bridge` `config.rs`, `std::env` | The bridge binary (Linux) | `[[domain]] locator` / `id` | Not measured (bridge binary not built) |
| Operational: `NROS_ENTRY_SPIN_MS`, `NROS_CENSUS_OUT`, `NROS_PROFILE_OUT`, `NROS_PROFILE_INPUTS`, `NROS_GRAPH_DUMP`, `NROS_RMW_TRACE_OPEN`, `NROS_RMW_CYCLONEDDS_SKIP_PREFIX`, `NROS_CYCLONE_MATCH_TIMEOUT_MS`, `XRCE_SERIAL_BAUD`, `NROS_BLESS_PROBE_SIDECAR` | Yes, each through its own reader | Linux | None (tooling and test controls), except `NROS_ENTRY_SPIN_MS`, which is also a build knob (`entry.spin_ms` in the knob registry) | `NROS_CENSUS_OUT`, `NROS_RMW_TRACE_OPEN`: Rust, C, C++ read, acted. `NROS_PROFILE_OUT`: Rust and C++ read (C did not). `NROS_ENTRY_SPIN_MS`: Rust read (the C and C++ examples are hand `main`s). `NROS_GRAPH_DUMP`, `NROS_BLESS_PROBE_SIDECAR`: - in a talker. The Cyclone and XRCE rows: not measured (no such image). `NROS_ENTRY_SPIN_MS` has no build reader (confirmed) |

`NROS_ENTRY_SPIN_MS` alone has seven run-time readers with four spellings of
the read: `std::getenv` in `rclcpp/detail/main.hpp`, `getenv` in
`nros_rtos_run_components.c`, `std::env::var` in `nros-board-linux` and
`nros-bridge`, `env_str` in `nros-board-threadx-linux`, `nros::env::var` in
`nros-cpp`, and `std::env` in the `nros::main!` expansion (re-counted on
`origin/main`).
