---
rfc: 0103
title: "One owner per fact: every configuration fact and every resource location
  has one authored home, and every road reads the resolved answer"
status: Draft
since: 2026-10
last-reviewed: 2026-10-10
implements-tracked-by: [phase-484]
supersedes: []
superseded-by: null
---

# RFC-0103 — One owner per fact

## Summary

A build needs two kinds of input it did not produce: **configuration facts**
(which board, which RMW, which domain, how big a pool) and **resource
locations** (where the FreeRTOS kernel, the cross compiler, the SystemModel
are). Two censuses on 2026-10-09/10 measured both, and found the same shape
twice: one fact, many places that state or derive it, each correct under the
configuration its author tested, all gates green
([issue 1767](../issues/1767-resource-location-resolved-many-ways.md)).

> **Every fact has exactly ONE authored home, chosen by who owns the fact.
> Every other place that carries it is GENERATED from that home. Every road
> (cargo, cmake, west) reads the resolved answer; none re-derives it.**

This RFC generalises three rules that each already hold for a slice:
RFC-0094 ("one place decides a knob"), RFC-0098 D10 ("`system.toml` is the
single source of an image's knobs, RMW and deploy endpoint", Zephyr road,
being implemented by phase-481) and RFC-0095 D4 ("store first, checkout
last", Zephyr workspace only). It closes the cmake/west exclusion of
RFC-0101 §6.

**Boundary with phase-481.** Phase-481 owns the Zephyr leaf conf migration,
the Kconfig fragment renderer, per-image workspace configures and issues
1721/1757. This RFC depends on that renderer and does not touch the example
`.conf` files it migrates.

## 1. What was measured

### 1.1 Locations — 13 mechanisms, 13 resources resolved 2+ ways

Mechanisms: env-first with re-root (cargo, and separately shell), cargo
`links`, marker walk, counted literal, three descriptor-token interpolators,
cargo `[env]`/`include`, cmake cache→ENV→default, `find_program`/PATH, the SDK
store (pin parsed four ways), ament search, three `OUT_DIR` JSON parsers, and
per-artifact path functions with cmake/shell mirrors.

Worst: FreeRTOS by 7 arms (81 `system.toml` `{env:}` rows read raw, no
re-root); the store root in 4 precedence orders over 3 variable names; the
nano-ros root by 5 ladders over 3 variable names and 2 marker files; Cyclone's
override reaching cargo and ignored by the Zephyr road. Full list in issue 1767.

### 1.2 Configuration — ~30 places, the same fact in up to 10

| fact | places |
| --- | --- |
| RMW | 6 in one leaf (`examples/zephyr/rust/talker`: `system.toml`, `prj-zenoh.conf`, snippet, `sample.yaml SNIPPET`, an inert cargo feature, `CMakeLists --features`), plus `NANO_ROS_RMW`, `NROS_RMW`, `--rmw`, `fixtures.toml` |
| board | `[image] board`, west `-b`, `--west-board`, `-DNANO_ROS_BOARD`, `nros::main!(board=)`, `NROS_BOARD`, cargo deps, `sample.yaml`, `boards/*.conf` names |
| domain id | `[system]`, `[image]`, `CONFIG_NROS_DOMAIN_ID`, `CONFIG_NROS_CYCLONE_DOMAIN_ID`, `ROS_DOMAIN_ID`, baked `NROS_DOMAIN_ID`, test `config.toml`; on Zephyr only Kconfig is load-bearing (issue 1550) |
| locator | `[image]`, Kconfig, `NROS_LOCATOR`/`ZENOH_LOCATOR`, baked `option_env!`, board `config.rs` literals, cmake `NROS_APP_CONFIG` defaults, `NROS_LEAF_LOCATOR` |
| IP / gateway | `[image]`, board `config.rs`, cmake `NROS_APP_CONFIG`, test `config.toml` |
| triple / link | `nros-board.toml cargo_config`, 23 leaf `.cargo/config.toml`, `[board.cmake] toolchain_file`, `cmake/toolchain/*` |

And one knob carries up to three NAMES: Kconfig `NROS_ZENOH_TX_BATCH` = env
`ZPICO_TX_BATCH` = TOML `[knobs.zenoh.tx] batch`; `NROS_MAX_SUBSCRIBERS` is
floored into `ZPICO_MAX_SUBSCRIBERS`; `[board.knobs.net]` becomes
`NROS_SMOLTCP_*` with a deprecated `ZPICO_SMOLTCP_*` alias. cmake mixes
`NANO_ROS_*` and `NROS_*`. Precedence differs by road (RFC-0049's ladder;
Zephyr's `_nros_resolve_knob` env > image > Kconfig > derived > default; a
descriptor's `force = true`; cmake cache before ENV).

## 2. Design

### D1 — Owners, and what each may state

| layer | owner | authored in | owns | never states |
| --- | --- | --- | --- | --- |
| 0 infra | this repo | `nros-sdk-index.toml` | resource pins, kinds, the one env name per resource | knob values |
| 1 platform | RTOS port | `nros-platform.toml` | RTOS capabilities, knob defaults, priority plan | paths, board facts |
| 2 board | board author | `nros-board.toml` + board-owned files (`memory.x`, `FreeRTOSConfig.h`, config dirs) | identity, triple, link, toolchain file, `west_board`, capabilities, knob defaults, **network defaults** | user choices; no `force` |
| 3 backend | RMW / serdes | `nros-rmw.toml`, `nros-serdes.toml` | link, capabilities | knob values |
| 4 package | node author | `package.xml`, `Cargo.toml`/`CMakeLists.txt`, `nros-codegen.toml`, `nros::node!` | build type, entities, message bounds | board, RMW, network |
| 5 project | integrator | **`system.toml`** + launch + `*.contract.yaml`; per developer, `.envrc` | every SELECTION (board, RMW, domain, locator, IP, transport), knob overrides, `[sources]` | — |
| generated | `nros` | `<build>/nros/` (D7) | the resolved answer and its provenance | anything hand-written |

There is **no machine-wide layer**. A file under `$NROS_HOME` that changes a
build would change every project on the host without notice (decided
2026-10-10). Per-developer settings live in the project's `.envrc`, which
direnv scopes to that directory.

### D2 — Single source in files; the command line is an override, not a source

- **Files**: one writable home per fact. A second file stating the same fact
  is refused, not ranked. Phase-481 W2's `check-leaf-conf-nros-knobs` is the
  first instance for tracked examples; D9 lists the others, and D2a makes
  Zephyr refuse it in a USER's project too.
- **Env and command line** (`NROS_*`, `--rmw`, `-DNROS_IMAGE=`): allowed, and
  they win, because one-off runs and fixture scripts need them. They are never
  a second SOURCE: the resolver reads them BEFORE rendering, so every road
  (`.config`, `autoconf.h`, cargo's env, the C defines) sees one value; each is
  printed at configure and recorded in `resolved.toml [provenance]` as
  `transient`. A `-DCONFIG_*` on a managed knob is not this channel (D2a).

### D2a — On Zephyr, the single source is enforced by Kconfig itself

Measured 2026-10-10 on Zephyr 3.7 AND 4.4 (`native_sim/native/64`; the
conditional-prompt row on 3.7 only), a scratch module with
the same `module_ext_root` hook phase-481 uses (configure only; values read
from `zephyr/.config`):

| knob symbol shape | no `system.toml`: `prj.conf` 9 / `-DCONFIG` 17 | with `system.toml` (13): `prj.conf` 9 / `-DCONFIG` 17 |
| --- | --- | --- |
| prompt (today) | 9 / 17 | the fragment's 13 is a conf ASSIGNMENT; ordering decides |
| conditional prompt `"…" if !MANAGED` | 9 / 17 | **13, with only a WARNING** (`was assigned the value '9' but got the value '13'`) — the user's line is silently ignored |
| **no prompt in the module; the hook ADDS one when there is no `system.toml`** | **9 / 17** | **ERROR**: `is not directly user-configurable (has no prompt)`, configure fails |

The third row is the design. The module's `Kconfig` defines every nano-ros
knob symbol WITHOUT a prompt and first `osource`s a file the
`module_ext_root` hook generates before Kconfig runs:

- **a project with `system.toml`** — the file carries the resolved values as
  `default`s (sourced first, so they win). No prompt exists anywhere, so a
  `prj.conf` line or a `-DCONFIG_*` for that symbol is a Kconfig ERROR naming
  the symbol: one source, enforced by Zephyr, in projects no gate of ours
  reads.
- **a project without one** (a plain Zephyr app using nano-ros as a module,
  RFC-0072's guest) — the file only ADDS `int "<prompt>"` to each symbol, so
  `prj.conf`, menuconfig and `-DCONFIG_*` work exactly as today. The guest's
  single source is its conf files.

A reconfigure without a wipe follows the generated file both ways (4.4:
13 → 11 → back to the guest default 5), because Kconfig parsed it and Zephyr
re-runs Kconfig when a parsed file changes.

A conditional prompt is rejected: it turns the conflict into a warning and
drops the user's value, which is the silent-ignore class this RFC exists to
remove. Values are delivered as Kconfig DEFAULTS rather than a conf fragment,
so phase-481's `EXTRA_CONF_FILE` ordering question disappears. The generated
file is parsed by Kconfig, so it is in Zephyr's own reconfigure inputs. The
error names the symbol's definition site; the hook names the generated file
`<build>/nros/Kconfig.from-system-toml` so that site reads as the answer.

### D2b — Zephyr: the split with phase-481

Phase-481 (RFC-0098 D10/D11) already renders an image into Zephyr's form: the
module's `module_ext_root` hook writes a conf FRAGMENT from `system.toml` and
places it last in `EXTRA_CONF_FILE`. It carries the RMW and API choices, the
endpoint (`ZENOH_LOCATOR`, `XRCE_AGENT_*`), `DOMAIN_ID`, and every `[image.<id>]
env` row with a Kconfig symbol. D2a replaces the fragment's KNOB rows with
Kconfig defaults; this section fixes which facts move, who builds what, and in
which order, so the two phases do not build the same carrier twice.

**Measured (2026-10-11), the facts the split turns on:**

- The module `Kconfig` defines 124 nano-ros symbols (80 `int`, 39 `bool`, 5
  `string`), every one with a prompt; 73 sit under `if NROS_RMW_<X>`.
- Zephyr exports `KCONFIG_BINARY_DIR` into the Kconfig environment on 3.7 and
  4.4 (`cmake/modules/kconfig.cmake`, `COMMON_KCONFIG_ENV_SETTINGS`), so the
  module can `osource "$(KCONFIG_BINARY_DIR)/…"` a file the hook writes — no
  new plumbing.
- The fixture drivers pass per-run values as `-DCONFIG_NROS_*`
  (`zephyr-fixture-leaves.sh:450-452`, `zephyr-fixture-run-one.sh:159-165`: the
  locator, the XRCE agent port, the Cyclone domain). Under D2a every such
  symbol becomes a configure ERROR in a project with a `system.toml`.
- 23 C/C++ sources read `CONFIG_NROS_*` directly (`ZENOH_LOCATOR` 13,
  `ZEPHYR_HEAP_SIZE` 8, `SNTP_*`, `DOMAIN_ID`, tier stacks, boot report, …), so
  an exported env override — which wins for cargo and the cmake knob lane —
  never reaches them. Env alone cannot replace `-DCONFIG`.
- The knob census (phase-484 W5, 2026-10-11): 95 knob env names (49 on the
  board/platform ladder, 46 env-only) plus 5 alias spellings; 37 are
  `ZPICO_*`. Every reader already keys on the env NAME (`nros_zephyr_build::
  knob`, `platform_config::resolve_*`); the TOML-key → env map is 11 hand
  `*_env_key` match functions (the tx knobs are literals); `KCONFIG_PAIRS`
  holds the 20 env ↔ Kconfig rows whose names differ (its own comment still
  says 19).

**The split.**

1. **By kind of fact.** KNOBS (pool sizes, buffers, stacks, timeouts — the
   values a user's `prj.conf` used to set) move to D2a: prompt-less in the
   module, defaults from the generated file, a conf assignment refused by
   Kconfig. The RMW and API CHOICES, the ENDPOINT and the DOMAIN stay in
   phase-481's fragment, which is measured on both lines; they move only if the
   open measurements below are green. This keeps the fixture drivers'
   endpoint `-DCONFIG_*` and the 4.x RMW snippets working unchanged.
2. **One knob registry, anchored on today's Kconfig names.** A committed
   registry row per knob: id, group, type, RMW condition, unit, builtin, range,
   prompt and help. The id IS today's `CONFIG_NROS_<ID>` stem, so no Kconfig
   symbol is renamed and no guest's `prj.conf` breaks on a rename. The env
   spelling is `NROS_<ID>`; today's other spellings (`ZPICO_*`, the
   `NROS_SMOLTCP_*`/`ZPICO_SMOLTCP_*` net group, params as `NROS_MAX_*`, the
   per-platform memory names) are legacy rows in the registry, accepted with a
   deprecation until W8. The TOML spelling is `[knobs.<group>] <name>`.
   `KCONFIG_PAIRS` and the 11 `*_env_key` functions are DERIVED from the
   registry, then deleted. The registry also generates the module `Kconfig`'s
   knob section (prompt-less) and the prompt text a guest needs; a gate holds
   the committed `zephyr/Kconfig` equal to the generated one.
3. **A command-line override channel.** With knob prompts gone,
   `-DCONFIG_<knob>` is an error for a `system.toml` project, and env cannot
   reach C code that reads `CONFIG_*`. So the hook accepts
   `-DNROS_OVERRIDE_<ID>=<v>` and renders it into the generated default with
   provenance `transient` (RFC-0103 D2: the command line is an override, not a
   source; W6's per-value provenance). Zephyr caches a `-DCONFIG_*` as
   `CLI_CONFIG_*` and re-applies it (phase-481 W0 correction 1), so a build
   directory that once passed one fails until it is `-U`'d; the refusal says so.

**Ownership.**

| piece | phase |
| --- | --- |
| the registry, its generator, mechanical env names + legacy rows, `KCONFIG_PAIRS` / `*_env_key` derived then deleted, the bypass readers routed through the ladder, the drift gate | 484 W5a (no Zephyr behaviour change) |
| measure M1–M4 below, on 3.7 and 4.4 | 481 |
| `Kconfig.from-system-toml` in the hook, the prompt-less knob section, the override channel, the fragment dropping knob rows | 481 |
| the fixture drivers' KNOB `-DCONFIG_*` move to the override channel (endpoint ones stay) | 481 |
| choices / endpoint / domain to D2a, if M1/M2 are green | joint, later |

**Order.** 484 W5a lands first — it is what the hook's generator reads. 481
then measures and builds the hook. Phase-484's W5 acceptance waits on that.

**Open measurements (M1–M4), each a configure on 3.7 and 4.4:**

- **M1** — a prompt-less NAMED choice (`choice NROS_RMW_BACKEND`) redefined in
  the sourced file with a `default`: does it select, and does a conf
  assignment of a member error out?
- **M2** — a `default` in the sourced file, OUTSIDE `if NROS_RMW_<X>`, for a
  symbol defined inside it: right value when the RMW is selected, no stray
  value when it is not?
- **M3** — `-DCONFIG_<prompt-less>` is a hard error on 4.4 as on 3.7.
- **M4** — a guest (no `system.toml`) configures and RE-configures with the
  prompts declared only in the generated file, and menuconfig shows them.

### D3 — One precedence order, every road

Configuration:

    builtin < platform < board < derived < project  ‖  env < CLI (transient)

Locations, first match wins:

    1. explicit argument (flag, `-D`, function arg)
    2. the resource's env var (incl. `.envrc`), re-rooted per issue 1280
    3. project `[sources]` / `[tools]` in `system.toml`, project-relative
    4. a LOCAL EDIT in this checkout (D5)
    5. the store, at the pin
    6. this checkout's submodule, at the pin
    7. fail, naming `nros setup <resource>`

Every road gets this order by READING `resolved.toml` (image builds) or by
calling the one resolver (D4) — never by implementing it.

**Board facts and `force`** (corrected by phase-484 W2b). A value a board must
own — its RTOS port, its `FreeRTOSConfig.h` / `tx_user.h` dir — is stated ONCE,
in its `nros-board.toml`, and reaches the compile through the generated cargo
config with `force = true`: the ambient environment may not replace a board
fact, and an image changes one through its own `[image.<id>] env` row (the
project rung). What the first draft called "delete `force`" was the wrong
target: `force` was never the defect, the SECOND home was — `just/sdk-env.just`
exported one repo-wide `FREERTOS_PORT=GCC/ARM_CM3` and mps2 config dir, so the
RISC-V board needed `force` to keep the linux config out and two cmake board
modules carried code refusing the inherited port. With the export gone the
descriptor is the only home, and `force` is what makes it so.

### D4 — One resolver, two front doors

- **Implementation**: `nros_build_paths`, reading resource rows from the index.
  Build scripts link it.
- **Everything else**: `nros locate <resource>` (approved 2026-10-10), a thin
  wrapper. `--why` prints every rung and which one answered; `--format
  cmake|sh|json` for batch use.
- **cmake / west**: configure runs `nros locate --format cmake` ONCE, writes
  `<build>/nros/locations.cmake`, adds the index and each answer's freshness
  input to `CMAKE_CONFIGURE_DEPENDS`. Locations are normal variables, never
  `CACHE`, so unsetting an override takes effect on the next configure.
- **Our tooling never EXPORTS a location into the user's shell.** (Scope
  stated precisely by phase-484 W2c: the defect is a value in an environment
  something ELSE inherits — a sourced shell, every child of it, every linked
  worktree started from it. A `just` recipe that computes `FREERTOS_DIR` for its
  own processes from its own checkout, per run, is inherited by nothing else
  and is not that defect.) `just/sdk-env.just`'s 21 path
  exports and `activate.sh`'s path exports are deleted; `activate.sh` puts
  `nros` on PATH and nothing else. An export is inherited by every child
  process and every linked worktree, which is issue 1280's root cause —
  generating the exports instead (the first draft of this design) keeps it.

```
$ nros locate freertos-kernel --why
freertos-kernel = ~/.nros/sources/freertos-kernel/10.6.2+8f4c1e2a
  1 arg         -
  2 env         FREERTOS_DIR unset
  3 project     no [sources] row in ./system.toml
  4 local edit  third-party/freertos/kernel not initialized
  5 store       pin 10.6.2+8f4c1e2a        <- chosen
```

### D5 — Store first, and a local edit beats the store

Source trees join tools in the store (decided 2026-10-10). The store already
has the category: `[source.rosidl]` is `location = "store"` and provisions to
`$NROS_HOME/sources/<name>/<version>` (`sdk_store::source_dir`). D5 extends
that to every row rather than adding a second spelling:

- **Path** `$NROS_HOME/sources/<name>/<version>+<sha8>/` — the sha because a
  patched fork keeps upstream's version. (As built, phase-484 W3c: the `+sha8`
  applies to SUBMODULE rows; a clone row such as `rosidl` keeps `<version>`,
  because its `ref` is the upstream release its version already names.)
- **Every `[source.*]` row states `ref = "<full sha>"`.** Today only rosidl
  does; a submodule row's sha lives in the superproject gitlink (checkout) or
  in `nros-submodule-pins.toml` written by `stage-sdk-root.sh` (install) —
  two owners of one fact. A gate asserts gitlink == `ref`, and
  `nros-submodule-pins.toml` retires. (Landed, phase-484 W3a:
  `check-source-refs`; an installed root reads its own index copy, recognised
  by having no `.git`.)
- **Materialised by `git archive <ref>`, hardlinking files unchanged from the
  nearest existing pin of the same source**, then `chmod a-w` (a shared inode
  must never be written). Measured on zenoh-pico (D5-M below): no `.git` in
  the tree, so issue 1336's layout trap cannot occur, and every mtime is the
  commit time and never moves, so the fixture treadmill stops for these trees.
- **A source a cargo `path =` dependency reaches (`nuttx-libc`, `px4-rs`), or
  one a build PATCHES in place (`cyclonedds-src`, by the Zephyr lane), also
  stays a checkout source** (phase-484 W3c): cargo resolves a path dependency
  at manifest parse, before any ladder runs, and a patched tree cannot be a
  read-only shared one.
- **A source whose build writes INTO its tree stays a checkout/workspace
  source**: measured, `nuttx-kernel` (674 build entries in-tree),
  `nuttx-apps` (273) and `px4-autopilot` (`$PX4_DIR/build/`). Every other row
  had zero ignored or dirty files in a checkout that builds it. A gate keeps
  build output out of any `sources/` tree.

Effects: an installed user (no checkout) builds; an agent worktree needs no
submodule init for the store-eligible rows; projects share one copy.

The one refinement to pure store-first: in a checkout, a submodule whose HEAD
differs from the pin, or that is dirty, is a contributor's edit in progress,
and it outranks the store (rung 4) — otherwise a fork developer silently builds
the store's stale copy, the museum-binary class. A checkout submodule AT the
pin is the same content as the store, so either answers; the store is
preferred. Every build prints the origin of each resource it used.

**D5-M, measured 2026-10-10.** Sizes in the main checkout (worktree / module
store): px4-autopilot 1174 M / 579 M, nuttx-kernel 536 M / 119 M, netxduo
283 M / 57 M, threadx 114 M / 9 M, the other ten under 45 M each; ~2.26 G of
trees in all. Churn over six months on `main`: 113 distinct source pins, 86 of
them zenoh-pico (63) and cyclonedds (25). One full tree per pin would cost a
contributor building every pin ~0.4–0.5 G a month. A second zenoh-pico pin
38 moves later (177 files changed) costs 7.3 M as a depth-1 clone, 6.1 M as a
`git worktree` over a shared bare repo (depth-1 packs do not delta), and
**3.2 M under archive + hardlink**; chained over all 38 consecutive pins,
24.6 M against ~228 M (9×). A Cyclone pin six months later costs 2.0 M against
21.9 M.

**Reclaim.** `store.rs` gains `Category::Sources` (depth 2) and a
`PinRule::Source { name, version, sha }`; provisioning writes
`.nros-provenance`. Liveness is mark-and-sweep inside the store: a source is
live if the index of any installed toolchain (`sdk/nros/<ver>/share/nano-ros/`)
or the checkout index `discover_pin_files` finds names its
`(name, version, sha)`; anything else falls to `--older-than`. Without this,
`scan` files `sources/` under `Other`, which gc never collects — and today's
exact-string `AnyVersion` match would treat a live `+sha8` directory as
unnamed.

**The new cost.** A pin move changes the source PATH, so the library it
builds recompiles in full and sccache misses (its key includes `-I` paths),
where a submodule bump today rewrites only the changed files. Accepted: pin
moves are a few a month outside zenoh-pico/cyclonedds, and those two are the
rows a contributor edits, which rung 4 serves from the checkout anyway.

### D6 — One name per root, one name per resource

- Store root: `NROS_HOME` only (`NROS_STORE`, `NROS_SDK_STORE` retire).
- nano-ros root: `NROS_REPO_DIR` only (`NANO_ROS_ROOT`, `nano_ros_ROOT`
  retire as root inputs; `nano_ros_ROOT` stays CMake's own `find_package`
  hint, but is no longer exported); one marker,
  `packages/core/nros-core/Cargo.toml` (`nros_build_paths::CHECKOUT_MARKER` —
  corrected by phase-484 W1: ~25 sites already used it, only two walks used
  the index, and both files sit at every SDK root).
- **Which index answers**: the one beside the crates being compiled. Every
  SDK root — a checkout, or an install's `share/nano-ros/` staged by
  `stage-sdk-root.sh` — carries `nros-sdk-index.toml` at its top, and every
  nros crate is compiled from inside one, so `nros_build_paths` keeps its walk
  up from `CARGO_MANIFEST_DIR` (D6-M). A lookup emits `rerun-if-changed` on
  that index; a store tree is content-keyed and immutable, so it is never
  watched.
- Each resource row in the index declares its one `env` name (existing names
  kept: `FREERTOS_DIR`, `CYCLONEDDS_SOURCE_DIR`, …); duplicates
  (`CYCLONEDDS_DIR`) retire.
- **Knobs** (extends RFC-0098 D10 beyond Zephyr): one canonical id
  `<group>.<name>`, every spelling MECHANICAL — TOML `[knobs.<group>] <name>`,
  env `NROS_<GROUP>_<NAME>`, Kconfig `CONFIG_NROS_<GROUP>_<NAME>`. No pairs
  table, no aliases. Backend-internal names (`ZPICO_*`, `XRCE_*`) become
  generated C macros, not user inputs.
- Manifest sections are spelled the same in every file: `[knobs]`,
  `[capabilities]`, `[net]` (the file kind already says the owner, so
  `[board.knobs]` → `[knobs]`).

### D7 — Generated output lives under `<build>/nros/`

| artifact | target path |
| --- | --- |
| resolve snapshot | `<build>/nros/resolved.toml` (gains `[locations]` + per-value provenance) |
| cargo config | `<build>/nros/cargo.toml` |
| cmake locations | `<build>/nros/locations.cmake` |
| config header | `<build>/nros/include/nros/nros_config_generated.h` (D8) |
| SystemModel | `<build>/nros/models/<bringup>/` |
| sizing descriptor | `<build>/nros/sizing/<entry>.toml` |

Each has one path function in Rust; cmake and shell ask `nros locate artifact
…` rather than mirroring it. Build output is never in the store (RFC-0095).

### D8 — The config header is written by resolve

RFC-0094 already decides knob values before configure, so the header can be
written then, before any compile — which deletes the three `OUT_DIR` JSON
parsers and the PX4 `${NANO_ROS_ROOT}/target/...` literal. Cargo dependents
keep `DEP_NROS_C_*` (RFC-0101 D2). Until resolve owns it, one parser in
`nros_build_paths` serves every non-cargo consumer.

**Measured (phase-484 W6b): the premise holds for half the header.** The knob
values are decided before configure, but the header also carries the opaque
STORAGE SIZES (`NROS_EXECUTOR_SIZE`, the `*_OPAQUE_U64S` family), which the
build script reads out of the compiled `nros` rlib (`nros-sizes-build`'s nested
cargo build). No step before that compile can know them, so resolve cannot
write the header. What is single instead is where it LANDS:
`nros_build_paths::artifact`, which the build scripts write through and
`nros locate artifact` reads — the PX4 literal is gone that way. The `OUT_DIR`
reader stays the interim for consumers that want the per-unit copy.

### D9 — Gates

- **No unrouted location**: a path-valued env read, a literal path or a
  `find_program` for a resource the index declares fails unless it goes
  through `nros_build_paths` or `nros locate` (widens issue 1560's gate to
  every road).
- **No second home**: extends phase-481's leaf-conf gate to `.cargo/config.toml
  [env]`, test `config.toml`, `CMakeLists --features` for RMW, and board
  `config.rs` network literals.
- **Mechanical names**: every knob's three spellings derive from its id.
- **Pins agree**: a checkout's gitlink equals its row's `ref`.
- **Store trees stay clean**: no build writes under `$NROS_HOME/sources/`.
- **Census completeness**: `nros-build-wiring.py` fails on any UNCLASSIFIED row
  (issue 1768).

## 3. Where each fact lives

| fact | home | rendered into | retired |
| --- | --- | --- | --- |
| board | `[image.<id>] board` | west `-b` (via `west_board`), cmake, cargo | `NROS_BOARD`, `main!(board=)`, `-DNANO_ROS_BOARD`; alias casing (match case-insensitively) |
| RMW | `[image.<id>] rmw` | Kconfig choice, snippet, cargo features | inert `rmw-*` features, `CMakeLists --features` (the conf lines are phase-481's) |
| domain id | `[system] domain_id`, `[image]` override | Kconfig (closes 1550), baked default | `NROS_DOMAIN_ID`, `CYCLONE_DOMAIN_ID` as inputs; `ROS_DOMAIN_ID` stays the runtime override |
| locator, IP, transport | `[image]` → board `[net]` default | Kconfig, baked default, cmake config | board `config.rs` literals, cmake `NROS_APP_CONFIG` defaults, `ZENOH_LOCATOR`; `NROS_LOCATOR` stays the runtime override |
| knobs | `[knobs]` (platform, board), `[image.<id>.knobs]` (project) | env / Kconfig / cmake by D6 | `ZPICO_*` as user names, the pairs table, leaf `.cargo [env]` |
| triple, link | `nros-board.toml` | `cargo.toml`, cmake toolchain | 23 leaf `.cargo/config.toml` |
| board config dirs | `nros-board.toml`, board-relative | — | `THREADX_CONFIG_DIR` `force`, `sdk-env.just` defaults |
| sources, tools | index pin + project `[sources]`/`[tools]` | `resolved.toml [locations]`, `locations.cmake` | 81 `{env:}` rows, 21 exports, 4 of 5 root ladders, 3 of 4 store orders, the cmake cache→ENV pattern (11 files), Zephyr's literal Cyclone path, `build/qemu` |
| entities, QoS | package + contract | sizing descriptor | — |

`[image.<id>] env` rows (RFC-0098's APP rung, used by phase-481) stay valid:
an env name maps mechanically to its canonical id under D6, so they become a
spelling of `[image.<id>.knobs]` and retire only after phase-481 closes.

### Resource target paths

`$NROS_HOME` defaults to `~/.nros`; `<pin>` is `<version>+<sha8>`.

| resource | kind | target | override | `nros locate` |
| --- | --- | --- | --- | --- |
| store | root | `$NROS_HOME` | `NROS_HOME` | `store` |
| nano-ros | root | enclosing checkout, else `$NROS_HOME/toolchains/<ver>/` | `NROS_REPO_DIR` | `nano-ros` |
| `nros` | tool | `$NROS_HOME/bin/nros` (shim) | `NROS_CLI` | `tool nros` |
| `nros-launch-resolve` | tool | toolchain `bin/` (checkout: `packages/cli/target/release/`) | `NROS_LAUNCH_RESOLVE` | `tool nros-launch-resolve` |
| cross gcc, zephyr-sdk, qemu, corrosion, ninja, make, cyclonedds, xrce-agent | tool | `$NROS_HOME/sdk/<tool>/<ver>/` | per row | `tool <name>` |
| `rmw_zenohd`, ROS | external | `NROS_RMW_ZENOHD` → `AMENT_PREFIX_PATH` → `/opt/ros/$ROS_DISTRO`; never stored | `NROS_RMW_ZENOHD` | `tool rmw_zenohd` |
| FreeRTOS, lwIP, ThreadX, NetX, Cyclone src, zenoh-pico, mbedTLS, XRCE, micro-CDR, px4-rs, nuttx-libc | source | `$NROS_HOME/sources/<name>/<pin>/` | per row (`FREERTOS_DIR`, …) | row name |
| NuttX kernel + apps, PX4-Autopilot | source, built in-tree | checkout submodule (writable); store-eligible only once built out-of-tree | `NUTTX_DIR`, `NUTTX_APPS_DIR`, `PX4_AUTOPILOT_DIR` | row name |
| Zephyr workspace | source | `$NROS_HOME/workspaces/zephyr/<ver>/` (RFC-0095) | `NROS_ZEPHYR_WORKSPACE` | `zephyr-workspace` |
| vendor SDK we cannot ship | user source | env or project `[sources]` only | e.g. `NV_SPE_FSP_DIR` | row name |

## 4. Migration

Each wave deletes what it replaces in the same change.

1. **Roots and the resolver.** `NROS_HOME`/`NROS_REPO_DIR`/one marker;
   `nros locate` + `--why` over today's rungs. No source moves yet.
2. **Stop exporting.** Delete the 21 exports; cmake and west read
   `locations.cmake`; `just` and scripts call `nros locate`.
3. **Sources into the store.** Index rows gain `env`/`kind`; `nros setup`
   gain `ref`; `nros setup` materialises `sources/` (archive + hardlink,
   read-only) and gc covers it; the local-edit rung; project `[sources]`; the 81 `{env:}`
   rows go.
4. **Selections.** Board, domain, locator, IP, transport have one home; board
   `config.rs` and cmake `NROS_APP_CONFIG` defaults move to `[net]`.
5. **Knob names.** Canonical ids, mechanical spellings, `ZPICO_*` internal.
   Coordinated with phase-481 (its fragment renderer consumes the names).
6. **Generated layout.** `<build>/nros/`; the header written by resolve; one
   `OUT_DIR` parser until then.
7. **Leftovers.** Test-bin `config.toml`, `[board.knobs]` → `[knobs]`,
   `NANO_ROS_*` cmake vars, issue 1768.

## 5. Alternatives rejected

- **A machine config file (`$NROS_HOME/config.toml`).** Changes every project
  on the host without notice. `.envrc` is per project.
- **Generate the shell exports from the index.** Keeps the export, and the
  export is what a linked worktree inherits (issue 1280).
- **Resolve per image only (`resolved.toml` alone).** Plain `cargo build`,
  `just check`, scripts and tests have no image, so they need the resolver
  anyway; the snapshot is a record, not the mechanism.
- **Keep per-road implementations and gate their agreement.** The fix-the-site
  pattern of issues 0500, 0616, 1025, 1280 and 1527.
- **Rank two file writers instead of refusing.** Precedence between two files
  owned by the same layer hides the conflict issue 1757 is an instance of.

## 6. Measured answers, and what stays open

**D6-M — where `nros_build_paths` finds the index outside a checkout.**
Measured, not a gap: user projects never take nros crates from crates.io or
git; generated `[patch.crates-io]` rows (`builder/cargo_config.rs`) point at
crate roots inside an SDK root, and the staged install root contains the
index at its top (`stage-sdk-root.sh`). So today's walk already answers
correctly for a user project, a checkout and a linked worktree (each
worktree's crates find that worktree's index — issue 1280's rule by
construction). Rejected: `include_str!` of the index (every index edit, 142 in
six months, reruns every dependent build script, and it only helps a
crates.io publish nano-ros does not do); reading the project's
`nros-toolchain.toml` pin (a build script cannot see the project root, and in a
checkout it would answer with the store's index while the checkout's is being
edited); an env variable set by `nros build` (plain cargo lacks it, and it is
inherited across worktrees — issues 1280, 0491).

Still open:

1. **Out-of-tree NuttX and PX4 builds.** Until they exist those three trees
   cannot be shared read-only; the RFC does not depend on it.
2. **Error text.** Kconfig's refusal names the symbol and its definition
   site, not `system.toml`; the hook should also print which `system.toml`
   line owns the knob. Wording, not mechanism.

## Changelog

- 2026-10-11 — D2b: the Zephyr split with phase-481. Knobs move to D2a
  through one registry anchored on today's `CONFIG_NROS_*` names; choices,
  endpoint and domain stay in 481's fragment until M1–M4 are measured; a
  `-DNROS_OVERRIDE_<ID>` channel replaces the knob `-DCONFIG_*`.
- 2026-10 — D8 corrected by measurement (phase-484 W6b): the config header's
  sizes come from the compiled rlib, so it stays a build product; its location
  is one function (`nros_build_paths::artifact`, `nros locate artifact`).
- 2026-10 — D2a (Zephyr enforcement), D5 (store layout, reclaim) and D6-M
  (index location) measured and folded in; `src/` corrected to the existing
  `sources/` category.
- 2026-10 — created from the 2026-10-09/10 location and configuration
  censuses (issue 1767). Decisions taken with the user: `nros locate` is
  acceptable; store first; no machine-wide config file; single source in files.
