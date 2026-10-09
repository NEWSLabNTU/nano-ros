---
rfc: 0103
title: "One owner per fact: every configuration fact and every resource location
  has one authored home, and every road reads the resolved answer"
status: Draft
since: 2026-10
last-reviewed: 2026-10-10
implements-tracked-by: []
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
  is refused by a gate, not ranked. Phase-481 W2's
  `check-leaf-conf-nros-knobs` is the first instance; D9 lists the others.
- **Env and command line** (`NROS_*`, `-DCONFIG_*`, `--rmw`): allowed, and they
  win, because one-off runs and fixture scripts need them. They are never a
  second SOURCE: each is printed at configure and recorded in
  `resolved.toml [provenance]` as `transient`, so an image built under one says
  so.

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
calling the one resolver (D4) — never by implementing it. `force` is deleted:
a value a board must own is a board fact under its own name, not an override.

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
- **Our tooling never EXPORTS a location.** `just/sdk-env.just`'s 21 path
  exports and `activate.sh`'s path exports are deleted; `activate.sh` puts
  `nros` on PATH and nothing else. An export is inherited by every child
  process and every linked worktree, which is issue 1280's root cause —
  generating the exports instead (the first draft of this design) keeps it.

```
$ nros locate freertos-kernel --why
freertos-kernel = ~/.nros/src/freertos-kernel/10.6.2+8f4c1e2a
  1 arg         -
  2 env         FREERTOS_DIR unset
  3 project     no [sources] row in ./system.toml
  4 local edit  third-party/freertos/kernel not initialized
  5 store       pin 10.6.2+8f4c1e2a        <- chosen
```

### D5 — Store first, and a local edit beats the store

Source trees join tools in the store (decided 2026-10-10), keyed by content:
`$NROS_HOME/src/<name>/<version>+<sha8>` — the sha because a patched fork
keeps upstream's version. Effects: an installed user (no checkout) builds; an
agent worktree needs no submodule init; projects share one copy.

The one refinement to pure store-first: in a checkout, a submodule whose HEAD
differs from the pin, or that is dirty, is a contributor's edit in progress,
and it outranks the store (rung 4) — otherwise a fork developer silently builds
the store's stale copy, the museum-binary class. A checkout submodule AT the
pin is the same content as the store, so either answers; the store is
preferred. Every build prints the origin of each resource it used.

### D6 — One name per root, one name per resource

- Store root: `NROS_HOME` only (`NROS_STORE`, `NROS_SDK_STORE` retire).
- nano-ros root: `NROS_REPO_DIR` only (`NANO_ROS_ROOT`, `nano_ros_ROOT`
  retire); one marker, `nros-sdk-index.toml`.
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

### D9 — Gates

- **No unrouted location**: a path-valued env read, a literal path or a
  `find_program` for a resource the index declares fails unless it goes
  through `nros_build_paths` or `nros locate` (widens issue 1560's gate to
  every road).
- **No second home**: extends phase-481's leaf-conf gate to `.cargo/config.toml
  [env]`, test `config.toml`, `CMakeLists --features` for RMW, and board
  `config.rs` network literals.
- **Mechanical names**: every knob's three spellings derive from its id.
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
| FreeRTOS, lwIP, ThreadX, NetX, NuttX, Cyclone src, zenoh-pico, mbedTLS, XRCE, micro-CDR, PX4, px4-rs | source | `$NROS_HOME/src/<name>/<pin>/` | per row (`FREERTOS_DIR`, …) | row name |
| Zephyr workspace | source | `$NROS_HOME/workspaces/zephyr/<ver>/` (RFC-0095) | `NROS_ZEPHYR_WORKSPACE` | `zephyr-workspace` |
| vendor SDK we cannot ship | user source | env or project `[sources]` only | e.g. `NV_SPE_FSP_DIR` | row name |

## 4. Migration

Each wave deletes what it replaces in the same change.

1. **Roots and the resolver.** `NROS_HOME`/`NROS_REPO_DIR`/one marker;
   `nros locate` + `--why` over today's rungs. No source moves yet.
2. **Stop exporting.** Delete the 21 exports; cmake and west read
   `locations.cmake`; `just` and scripts call `nros locate`.
3. **Sources into the store.** Index rows gain `env`/`kind`; `nros setup`
   provisions `src/`; the local-edit rung; project `[sources]`; the 81 `{env:}`
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

## 6. Open questions

1. **Zephyr enforcement.** Can phase-481's fragment set promptless symbols so
   that a leaf conf assignment is a Kconfig error rather than a gate finding?
   Zephyr documents promptless assignments as errors; not measured here (no
   Zephyr workspace on this host).
2. **Store growth.** A content-keyed `src/` accumulates one tree per fork
   commit; RFC-0095 D11's reclaim must cover it.
3. **Installed crates.** Where `nros_build_paths` finds the index outside a
   checkout — embedded at build time, or the toolchain's copy.

## Changelog

- 2026-10 — created from the 2026-10-09/10 location and configuration
  censuses (issue 1767). Decisions taken with the user: `nros locate` is
  acceptable; store first; no machine-wide config file; single source in files.
