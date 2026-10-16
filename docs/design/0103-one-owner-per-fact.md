---
rfc: 0103
title: "One owner per fact: every configuration fact and every resource location
  has one authored home, and every road reads the resolved answer"
status: Stable
since: 2026-10
last-reviewed: 2026-10-15
implements-tracked-by: [phase-484, phase-497]
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
([issue 1767](../issues/archived/1767-resource-location-resolved-many-ways.md)).

> **Every fact has exactly ONE authored home, chosen by who owns the fact.
> Every other place that carries it is GENERATED from that home. Every road
> (cargo, cmake, west) reads the resolved answer; none re-derives it.**

This RFC generalises three rules that each already hold for a slice:
RFC-0094 ("one place decides a knob"), RFC-0098 D10 ("`system.toml` is the
single source of an image's knobs, RMW and deploy endpoint", Zephyr road,
being implemented by phase-481) and RFC-0095 D4 ("store first, checkout
last", Zephyr workspace only). It closes the cmake/west exclusion of
RFC-0101 §6.

**Boundary with phase-481.** Phase-481 (archived 2026-10-11) built the Zephyr
leaf conf migration, the Kconfig fragment renderer, per-image workspace
configures and issues 1721/1757. This RFC depends on that renderer and does
not touch the example `.conf` files it migrated. The D2b hook it was also
assigned is phase-484 W5b now.

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
| RMW | 6 in one leaf (`examples/zephyr/rust/talker`: `system.toml`, `prj-zenoh.conf`, snippet, `sample.yaml SNIPPET`, an inert cargo feature, `CMakeLists --features`), plus `NANO_ROS_RMW` (retired), `NROS_RMW`, `--rmw`, `fixtures.toml` |
| board | `[image] board`, west `-b`, `--west-board`, `-DNANO_ROS_BOARD` (retired), `nros::main!(board=)`, `NROS_BOARD`, cargo deps, `sample.yaml`, `boards/*.conf` names |
| domain id | `[system]`, `[image]`, `CONFIG_NROS_DOMAIN_ID`, `CONFIG_NROS_CYCLONE_DOMAIN_ID`, `ROS_DOMAIN_ID`, baked `NROS_DOMAIN_ID`, test `config.toml`; on Zephyr only Kconfig is load-bearing (issue 1550) |
| locator | `[image]`, Kconfig, `NROS_LOCATOR`/`ZENOH_LOCATOR`, baked `option_env!`, board `config.rs` literals, cmake `NROS_APP_CONFIG` defaults, `NROS_LEAF_LOCATOR` |
| IP / gateway | `[image]`, board `config.rs`, cmake `NROS_APP_CONFIG`, test `config.toml` |
| triple / link | `nros-board.toml cargo_config`, 23 leaf `.cargo/config.toml`, `[board.cmake] toolchain_file`, `cmake/toolchain/*` |

And one knob carries up to three NAMES: Kconfig `NROS_ZENOH_TX_BATCH` = env
`ZPICO_TX_BATCH` = TOML `[knobs.zenoh.tx] batch`; `NROS_MAX_SUBSCRIBERS` is
floored into `ZPICO_MAX_SUBSCRIBERS`; `[board.knobs.net]` becomes
`NROS_SMOLTCP_*` with a deprecated `ZPICO_SMOLTCP_*` alias. cmake mixed
`NANO_ROS_*` (since retired) and `NROS_*`. Precedence differs by road (RFC-0049's ladder;
Zephyr's `_nros_resolve_knob` env > image > Kconfig > derived > default; a
descriptor's `force = true`; cmake cache before ENV).

## 2. Design

### D1 — Owners, and what each may state

| layer | owner | authored in | owns | never states |
| --- | --- | --- | --- | --- |
| 0 infra | this repo | `nros-sdk-index.toml` | resource pins, kinds, the one env name per resource | knob values |
| 1 platform | RTOS port | `nros-platform.toml` | RTOS capabilities, knob defaults, priority plan | paths, board facts |
| 2 board | board author | `nros-board.toml` + board-owned files (`memory.x`, `FreeRTOSConfig.h`, config dirs) | identity, triple, link, toolchain file, `west_board`, capabilities, knob defaults, **network interfaces** (D10) | user choices; no `force`; a network's subnet or an image's address |
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
re-runs Kconfig when a parsed file changes. (On 3.7 a switch from a
system.toml rendering BACK to the guest file keeps the last value until a
pristine build; 4.4 returns to the module default — measured in W5b's M4.)

A conditional prompt is rejected: it turns the conflict into a warning and
drops the user's value, which is the silent-ignore class this RFC exists to
remove. Values are delivered as Kconfig DEFAULTS rather than a conf fragment,
so phase-481's `EXTRA_CONF_FILE` ordering question disappears. The generated
file is parsed by Kconfig, so it is in Zephyr's own reconfigure inputs. The
hook names the generated file `<build>/nros/Kconfig.from-system-toml`.
Kconfig's own error names only the MODULE's definition site (W5b, M3), so the
hook refuses a knob `-DCONFIG_*` or conf line itself first, naming
`system.toml` and the override channel.

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
2. **One knob registry; Kconfig symbols are not renamed.** A committed
   registry row per knob: id, group, type, RMW condition, unit, builtin,
   range, prompt, help, and the Kconfig symbol it reads. The env spelling is
   `NROS_<ID>`. A knob that belongs to one RMW is spelled `NROS_<RMW>_<X>`
   (`NROS_ZENOH_MAX_PUBLISHERS`), the shape `NROS_XRCE_*`, `NROS_CYCLONEDDS_*`
   and `NROS_ZENOH_TX_BATCH` already have. Today's other spellings
   (`ZPICO_*`, `ZPICO_SMOLTCP_*`, ThreadX's `NROS_ZENOH_READ_PRIORITY` beside
   `ZPICO_READ_TASK_PRIORITY`) were legacy rows, accepted with a deprecation
   for one release and RETIRED in phase-484 W8: set, each is refused naming
   `NROS_<ID>`, and the `legacy` column stays as that error table for good
   (2026-10-14): without it a retired spelling would be ignored, not refused. The TOML spelling is `[knobs.<group>] <name>`.

   The Kconfig symbol is a COLUMN, not the id. Its default is
   `CONFIG_NROS_<ID>`; a row names another symbol where today's Kconfig
   differs, so no guest's `prj.conf` breaks on a rename. The id cannot be
   the Kconfig stem, and this was measured, not assumed: the Zephyr lane
   resolves `NROS_MAX_PUBLISHERS` (the image's DEMAND, RMW-agnostic) and
   `ZPICO_MAX_PUBLISHERS` (the zenoh pool, the demand after its floor — issues
   1015/1033) as two env names holding different values, and forwards both
   to cargo (`zephyr/cmake/nros_cargo_build.cmake`, the
   `_nros_c_array_pool_floor` calls). Both read `CONFIG_NROS_MAX_PUBLISHERS`.
   Naming the pool after that symbol would hand it the unfloored demand,
   which is 1015's defect. 14 of `KCONFIG_PAIRS`' 20 rows differ from their
   symbol by the `ZPICO_` prefix alone, so a stem rule would rename all 14
   onto the `NROS_*` namespace where that collision lives.

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
| measure M1–M4 below, on 3.7 and 4.4 | 484 W5b |
| `Kconfig.from-system-toml` in the hook, the prompt-less knob section, the override channel, the fragment dropping knob rows | 484 W5b |
| the fixture drivers' KNOB `-DCONFIG_*` move to the override channel (endpoint ones stay) | 484 W5b |
| choices / endpoint / domain to D2a, if M1/M2 are green — M1 measured RED (2026-10-12), so they stay | — |

These rows read "481" until 2026-10-12: phase-481 archived without them and
they were folded back into phase-484 W5b.

**Order.** W5a lands first — it is what the hook's generator reads. W5b then
measures and builds the hook.

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
- nano-ros root: **the `nros` binary knows its own** (amended 2026-10-12).
  A released `nros` reads the SDK root staged beside it
  (`<prefix>/share/nano-ros`); an in-tree `nros` reads the checkout it was
  built in (a walk up from `current_exe` to the marker). That makes a
  contributor's process the same as a user's: neither exports a root, and
  `activate.sh` puts `nros` on PATH and nothing else. The ladder is: the
  explicit `--nano-ros-path` / `-Dnano_ros_ROOT`, then the walk-up from the
  workspace (a workspace inside a checkout builds that checkout), then the
  binary's own root. `$NROS_REPO_DIR` and an exported `nano_ros_ROOT` retire
  as inputs; `nano_ros_ROOT` stays CMake's `find_package` hint, which
  `nros build` and `nros init`'s preset pass explicitly. A cmake module
  locates the root from its own path (`CMAKE_CURRENT_LIST_DIR`), never from
  the environment. There is no chicken-and-egg: building the CLI
  (`just setup-cli`) is a `cargo build` whose scripts find the checkout from
  `CARGO_MANIFEST_DIR`. `NANO_ROS_ROOT` is retired as before. One marker,
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
  `[board.knobs]` → `[knobs]`). **Landed for `[knobs]` (phase-484 W5):** a board descriptor's knob
  section is the top-level `[knobs]` (`[knobs.<group>]`), applying to every
  `[[board]]` in the file. The per-entry `[board.knobs]` was read with a
  deprecation notice for one release and is RETIRED (phase-484 W8): both
  parsers (`BoardKnobsFile`, the CLI's `BoardFile`) refuse it, naming
  `[knobs]`.

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
parsers and the PX4 `${NANO_ROS_ROOT}/target/...` literal (a retired name). Cargo dependents
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

### D10 — Networks: the board has interfaces, the project has networks, the image has addresses

Added 2026-10-12. D1 first gave the board "network defaults", one identity
per board. A bridge breaks that: many boards, or many instances of one board,
share ONE network, and a board can sit on several networks (a QEMU run, a lab
bridge, a docker network) or have several NICs. Measured on the tree, the
QEMU boards had already split one fact three ways: their `Config` presets
said `192.0.3.10`, their entry paths said `10.0.2.30` / `.40` / `.50`, and
`mps2-an385` carried a third network behind a `docker` feature. The fact has
three owners:

| fact | example | owner | authored in |
| --- | --- | --- | --- |
| **interfaces** — the network hardware | one LAN9118, two NICs, a MAC prefix | board | `nros-board.toml` `[board.net]` |
| **network** — subnet, gateway, how it is realised | `10.0.2.0/24` via SLIRP; `192.0.3.0/24` on bridge `qemu-br` | project | `system.toml` `[network.<name>]` |
| **address** — this image's IP / MAC on a network | talker `.10`, listener `.11` | image | `[image.<id>.net.<ifname>]` |

- **One default network for every QEMU board**: `qemu`, QEMU's own user-mode
  (SLIRP) network exactly as QEMU configures it for a plain `-nic user` —
  `10.0.2.0/24`, gateway (and host) `10.0.2.2`, first guest `10.0.2.15`. No
  launcher passes `net=`/`host=` for it, and an image someone runs by hand
  with a plain `-nic user` reaches its router. It is the network every QEMU
  board but one family already ran on; the FreeRTOS C/C++ images alone ran
  `192.0.3.0/24` behind a launcher that reconfigured SLIRP to match, so they
  moved. A board's router default is derived from its gateway, so it is
  `tcp/10.0.2.2:<port>` on every QEMU board and every road. `192.0.3.0/24`
  stays what the TAP/bridge setup scripts (`scripts/qemu/setup-network.sh`)
  already make it: a bridge network, and the platform default
  (`BaseConfig::default()`) for a board that names no network (real
  hardware, the threadx-linux host simulation).
- **A bridge** is a network many images name; each states its own address,
  and the resolver refuses a duplicate address or MAC. That turns the
  "identical IP+MAC, identical zenoh ZID, silent no-delivery" trap into a
  build error.
- **A multi-NIC board** gets one `[image.<id>.net.<ifname>]` per interface.
- **A deployment network** (mps2's `docker`) is a `[network.*]` row, not a
  board feature.

Staged: phase-484 W4b-2 moves every QEMU board onto the one default network
and keeps the existing per-image last-octet override. The `[network.*]` table,
bridge/TAP launch, multi-NIC and the collision refusal are
[phase-497](../roadmap/archived/phase-497-networks-have-owners.md).

#### D10 amended 2026-10-15 — physical networks, DHCP, interface kinds, host-OS boards

The text above was written from the QEMU lanes, and read against two
deployments it does not cover, it leaves six questions open: a multi-port box
running a Linux bridge with real boards plugged into it, and a board whose
network driver (or whole transport) is the user's own. The amendment answers
them without moving any owner.

**D10a — A network says what it IS; how an emulator ATTACHES is separate.**
`mode = slirp | bridge | tap | none` mixed the two: for a physical board on a
lab switch, nano-ros neither creates the network nor attaches anything to it.
`[network.<name>]` states the network's own facts —

| key | meaning |
| --- | --- |
| `subnet` | CIDR, e.g. `192.168.50.0/24` (required, except on the built-in `qemu`) |
| `gateway` | the subnet's default route, if any |
| `router` | the zenoh/XRCE locator an image on this network uses by default (D10d) |
| `dhcp` | `true` when the network hands out leases (dnsmasq on the box, a lab DHCP server) |
| `attach` | how an EMULATED image joins it: `"slirp"`, `{ bridge = "<host dev>" }` or `{ tap = "<host dev>" }`. Absent ⇒ physical boards only, and an emulated image naming the network is refused at sync |

— and the built-in `qemu` network is `subnet = 10.0.2.0/24`, `gateway =
router-host = 10.0.2.2`, `attach = "slirp"`, `dhcp = false`. A lab bridge is
`[network.lab] subnet = …, gateway = …, router = "tcp/192.168.50.1:7447"` with
no `attach`; QEMU guests can join the same bridge by adding
`attach = { bridge = "br0" }`, and then real boards and guests share one
address plan, one collision check and one router. *Built 2026-10-16
(phase-497 W3):* `attach` reaches the launcher through the resolved network,
and ONE renderer (`nros_board_common::image_net::qemu_nic`, also behind
`nros ws qemu-nic`) turns it into QEMU's `-nic`; a network with no `attach`
refuses an emulated image there.

**D10b — An address is static or leased, and a MAC is always unique.**
`[image.<id>.net.<if>]` takes `network`, then exactly one of `address` (full
IPv4), `host` (last octet within `subnet`) or `address = "dhcp"`, and an
optional `mac`. *Staged 2026-10-15:* until W2's resolver, `network` is
required next to an address — the `nros::main!` reader has no board catalog,
so a board-default network taken by the CLI alone would resolve one
`system.toml` two ways. *Landed 2026-10-16 (W2):* the macro stopped resolving
— it bakes the CLI's answer (D10f) — so `network` defaults to the board's
again, and an interface on `qemu` with no address takes `10.0.2.15`, QEMU's
first guest. Rules, all refused at sync naming both images:

- two STATIC addresses equal on one SHARED network, or one equal to its
  `gateway`, or one outside its `subnet`. `qemu` is never shared: every
  `-nic user` makes a private SLIRP copy per VM, so two images on it do not
  meet;
- two MACs equal on one network — leased interfaces included, because a DHCP
  server keys its lease on the MAC, so two equal MACs collide whatever the
  address mode;
- `address = "dhcp"` on a network without `dhcp = true`.

An unstated `mac` is DERIVED, never copied from a board constant: the board's
MAC prefix (`[board.net] interfaces.<if>.mac_prefix`, else `02:00:00`) plus a
suffix from `(system name, image id, network, interface)`. Today's trap was
exactly a board constant shared by two images; deriving per image removes it
rather than asking every author to remember. *Amended 2026-10-16 (W2):* the
key gained the system's name, because an image id is unique only within its
project — every single-package FreeRTOS leaf calls its image
`mps2-an385-freertos`, and a talker/listener pair is two projects.

**D10c — An interface has a KIND, and only IP kinds take an address.**
`[board.net.<if>] kind = "ethernet" | "wifi" | "serial" | "custom"`. Ethernet
and Wi-Fi interfaces get addresses. A `serial` interface (`zpico-serial`, IVC)
or a `custom` one (the `nros_transport_ops_t` vtable a user installs, a link
nano-ros does not model) gets none: `[image.<id>.net.<if>]` on it is refused,
and its endpoint is the image's `locator` (`serial/…`, `custom://…`), which is
already where it lives. A board crate with its OWN Ethernet or Wi-Fi driver —
in-tree or out-of-tree — states the interface like any other and reads the
resolved answer through D10f; it never re-derives an address.

**D10d — The router default is derived on `qemu` only.** There the guest's
gateway is QEMU's host alias, so `tcp/10.0.2.2:<port>` is right on every road.
On any other network the router is usually a host on the subnet that is NOT
the gateway (the bridge box itself), so the default is that network's
`router`, else the image's `locator`, else a sync error naming both keys.
The platform fallback (`tcp/192.0.3.1:7447`, `BaseConfig::default()`) is no
longer reached by an image that names a network. *Built 2026-10-16 (W2), as
a precedence:* the image's own `locator` (`[image.<id>]` >
`[image_defaults]`), else the `router` of the network its first interface
sits on, else `[system] locator`; an image whose first network is not `qemu`
and reaches none of the three is refused. A network is more specific than the
project, so its router outranks `[system] locator`; an image's own statement
outranks both.

**D10e — Host-OS boards own no network facts.** On a board whose network
stack is the host operating system — `linux`, `threadx-linux`,
`freertos-posix`, Zephyr `native_sim` (NSOS) — the kernel owns interfaces,
addresses and routes, and a nano-ros image only opens sockets
(threadx-linux's NSOS shim forwards NetX's BSD calls to the host and ignores
the IP fields it is handed). `[image.<id>.net.*]` on such a board is refused
naming that reason. The board says so: `[board.net] stack = "host"` (phase-497
W1). *Corrected 2026-10-15:* this text first proposed reading the predicate off
`supported_netstacks = []`, which is wrong — NuttX and Zephyr list `[]` too,
because their OS brings its own stack — so the statement is explicit, on
`linux`, `freertos-posix` and threadx-linux. In particular a Linux box that BRIDGES
its ports is configured with Linux tools, not with `system.toml`; what
`system.toml` describes is the network the box makes, for the boards on it.
Forwarding between two NICs inside one image is likewise out of scope: W4's
multi-NIC is about which interface each address lives on, and which
interface a session binds is the middleware's (phase-206).

**D10f — One output, per interface, that every consumer reads.** The
resolver's answer for an image is
`(interface) -> { kind, ip | dhcp, netmask, gateway, mac }`, rendered once
into `<build>/nros/net/<image>.toml` (D7) and from there into the three
carriers that exist today: the Rust `BOARD_NET_*` constants
(`nros_board_common::board_net::emit_for_crate`, now per interface), the C
`nros_app_config_t.network[]` initializer, and the cmake board module's
variables. A custom driver reads one of those, by interface name; none of
them is a place to state a value. *Built 2026-10-16 (W2):* the carrier is ONE
text, `NROS_IMAGE_NET` (`nros_board_common::image_net`, `no_std` decoder, one
encoder): a cargo `[env]` row on both cargo roads, `-DNROS_IMAGE_NET` on the
workspace cmake road, the top-level leaf's `nros ws leaf-system` row on the
standalone cmake road. `nros::main!` bakes it verbatim into
`DeployOverlay::net` (the macro resolves nothing, so it needs no board
catalog); `nros board net --image-net` renders the C initializer from it; and
the one board whose C startup bakes the address before the kernel runs
(riscv64's `NROS_APP_CONFIG`) reads it in its build script. A second
interface rides the same text (W4); a single-NIC board applies the first.

**D10g — The single-interface spellings retire.** `[image.<id>] ip` /
`gateway` / `netmask` (38 tracked `system.toml` files on 2026-10-15) and
`[board.net] ip` / `netmask` / `gateway` / `mac` were the one-NIC form of
these facts. Phase-497 W1 migrates every tracked use and then refuses each,
naming its home: an image's address goes to `[image.<id>.net.<if>]`, a
gateway or netmask to `[network.<name>]`, a MAC to the image (or is
derived). *Staged 2026-10-15:* W1 retired the IMAGE keys; the BOARD keys
retire in W2 with `mac`, because the board default they hold has copies a
descriptor edit cannot reach (the riscv64 board's C `cfg_ip`, its cmake
`NROS_APP_NET_IP_LAST` default), and only W2's one resolver feeds those.
*Landed 2026-10-16 (W2):* the board keys are refused when the descriptor is
read, and the copies they fed are gone with them — the C statics are zero
until `nros_threadx_set_config` runs, and the 23 fixture rows that passed
`NROS_ENTRY_IP_LAST` / `NROS_APP_NET_{IP,MAC}_LAST` state `host` (and, for the
riscv64 Cyclone pair whose launcher pins the virtio MAC, `mac`) in the leaf's
own `system.toml`.
`[board.net] network` stays: it is the board's DEFAULT network for
an image that names none, which is how a QEMU board lands on `qemu` without
a line of `system.toml`.

IPv4 only. IPv6 and Wi-Fi credentials are not network-address facts and are
not in D10.

## 3. Where each fact lives

| fact | home | rendered into | retired |
| --- | --- | --- | --- |
| board | `[image.<id>] board` | west `-b` (via `west_board`), cmake, cargo | `NROS_BOARD`, `main!(board=)`, `-DNANO_ROS_BOARD` (retired); alias casing (match case-insensitively) |
| RMW | `[image.<id>] rmw` | Kconfig choice, snippet, cargo features | inert `rmw-*` features, `CMakeLists --features` (the conf lines are phase-481's) |
| domain id | `[system] domain_id`, `[image]` override | Kconfig (closes 1550), baked default | `NROS_DOMAIN_ID`, `CYCLONE_DOMAIN_ID` as inputs; `ROS_DOMAIN_ID` stays the runtime override |
| locator, transport | `[image]` → platform default | Kconfig, baked default, cmake config | board `config.rs` literals, `ZENOH_LOCATOR`; `NROS_LOCATOR` stays the runtime override |
| network, address | `[network.<name>]` + `[image.<id>.net.<if>]` (D10); interfaces in `[board.net]` | baked default, cmake `NROS_APP_CONFIG`, the QEMU launcher's NIC | per-board subnets (`10.0.2.x` entry defaults), cmake `NROS_APP_CONFIG` literals, the mps2 `docker` feature |
| knobs | `[knobs]` (platform, board), `[image.<id>.knobs]` (project) | env / Kconfig / cmake by D6 | `ZPICO_*` as user names, the pairs table, leaf `.cargo [env]` |
| triple, link | `nros-board.toml` | `cargo.toml`, cmake toolchain | 23 leaf `.cargo/config.toml` |
| board config dirs | `nros-board.toml`, board-relative | — | `THREADX_CONFIG_DIR` `force`, `sdk-env.just` defaults |
| sources, tools | index pin + project `[sources]`/`[tools]` | `resolved.toml [locations]`, `locations.cmake` | 81 `{env:}` rows, 21 exports, 4 of 5 root ladders, 3 of 4 store orders, the cmake cache→ENV pattern (11 files), Zephyr's literal Cyclone path, `build/qemu` |
| entities, QoS | package + contract | sizing descriptor | — |

`[image.<id>] env` rows (RFC-0098's APP rung, used by phase-481) stay valid:
an env name maps mechanically to its canonical id under D6, so they become a
spelling of `[image.<id>.knobs]` and retire only after phase-481 closes.
**Built (phase-484 W8):** `[image.<id>.knobs.<group>] <name>` is parsed on
every road, validated against the registry (key, type, range) and rendered
into the same rows an `env` entry states, so it needs no carrier of its own.
The key is the row's `toml` key, else its id split at the first `_`. An
`env` row naming a registry knob is refused, naming the knobs spelling
(release 0.6.0 carried it as a deprecation warning). Env rows the registry
does not hold stay valid.

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
   `NANO_ROS_*` cmake vars (retired, phase-484 W8), issue 1768.

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
2. ~~**Error text.**~~ Answered by W5b (M3): Kconfig's refusal names ONE
   definition, the module's, so the hook refuses a `-DCONFIG_<knob>` and a
   conf-file knob line itself first, naming `system.toml`, the `env` row and
   `-DNROS_OVERRIDE_<ID>`; Kconfig's error stays the backstop for files the
   hook does not read (board fragments, snippets).

## Changelog

- 2026-10-15 — Phase-497 W1 landed D10's schema. Three corrections from
  implementing it: the host-OS predicate is `[board.net] stack = "host"`, not
  `supported_netstacks = []` (D10e); the board-level keys retire in W2, not W1
  (D10g); and `network` is required next to an address until W2 (D10b).

- 2026-10-15 — D10 amended (D10a–D10g) for physical networks and custom
  drivers: a network states what it is and, separately, how an emulator
  attaches (`attach`, absent for a physical-only network); addresses may be
  leased (`dhcp`) and MACs are derived per image and checked unique; an
  interface has a `kind`, and serial/custom ones take no address; the router
  default is derived only on `qemu`; host-OS boards own no network facts; one
  per-interface output feeds every carrier; the single-NIC `ip`/`gateway`/
  `netmask` spellings retire in phase-497 W1.

- 2026-10-14 — An `[image.<id>] env` row naming a registry knob is refused
  (phase-484 W8), after release 0.6.0 shipped its deprecation.
- 2026-10-14 — The retired-name error tables (W8a's cmake names, W8b's
  `legacy` column and `[board.knobs]`, W8c's runtime aliases) are kept for
  good instead of dropped after one release: a dropped row makes an old name
  silently ignored, not refused.

- 2026-10-13 — `[image.<id>.knobs]` built (phase-484 W8): the project's knob
  home of the §3 table, rendered into the `env` rows' carrier on every road;
  an `[image.<id>] env` row naming a registry knob is deprecated.

- 2026-10-13 — Phase-484 W8b retired the legacy knob spellings and
  `[board.knobs]`: each is now an error naming its replacement, the registry's
  `legacy` column kept one release as the error table.

- 2026-10-13 — **Stable.** Phase-484 W1–W7 implemented D1–D9 as amended
  above; issue 1767 archived with a per-resource resolution, and the
  three locations it still found more than one way (host-tool binaries, QEMU,
  the generated-header names) were fixed as issues 1850, 1851 and 1852. D10's network tables, bridge and multi-NIC boards are phase-497.
  Old names (legacy knob spellings, `[board.knobs]`, the `NANO_ROS_*` cmake (retired)
  aliases, `nros::main!(board =)`) retire in phase-484 W8, after a release.

- 2026-10-12 — D6 amended: the `nros` binary knows its own SDK root (a
  release's `share/nano-ros`, an in-tree build's checkout), so `activate.sh`
  exports no root and a contributor's process matches a user's. D10 added:
  network interfaces (board), networks (project) and addresses (image) are
  three facts with three owners; every QEMU board shares one default network.

- 2026-10-12 — D2b measured (phase-484 W5b), and built. M1 red on 3.7 and
  4.4 — a prompt-less `choice` aborts configure before any assignment is
  read — so choices, endpoint and domain stay in the fragment. M2 green only
  when the generated default repeats the module definition's `if` and
  `depends on` (outside them it leaks a value into an image whose RMW leaves
  the symbol undefined), which is how the hook renders it. M3 green, with a
  correction to D2a: the error does not name the generated file, so the hook
  words the refusal (§6 item 2). M4 green, including what menuconfig shows.
  The guest's prompts are a committed render (`zephyr/Kconfig.knob-prompts`),
  copied by the hook, so a guest needs no CLI.

- 2026-10-12 — D2b ownership: the Zephyr hook (M1–M4, `Kconfig.from-system-
  toml`, the prompt-less knob section, the override channel) moves from
  phase-481, which archived without it, to phase-484 W5b. Design unchanged;
  premises re-checked against the tree.

- 2026-10-11 — D2b: the Zephyr split with phase-481. Knobs move to D2a
  through one registry anchored on today's `CONFIG_NROS_*` names; choices,
  endpoint and domain stay in 481's fragment until M1–M4 are measured;
  RMW-specific knobs are spelled `NROS_<RMW>_<X>` and the Kconfig symbol is a
  per-row column (the zenoh pools and the demand they floor share one); a
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
