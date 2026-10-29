# The canonical build path

**What this is.** A map of how a nano-ros image actually gets built: the three
roads, what carries a configuration knob to the compiler on each, where a
build finds a resource it did not produce, and which RFC owns each decision.
It exists because the decisions are all made and all scattered — several RFCs
own a slice each, and nobody owned the picture. The rule that ties them
together is RFC-0103's: **every fact has one authored home, every other copy
is generated from it, and every road reads the resolved answer.**

**What this is not.** A new decision. Every rule here is already settled
somewhere; this file points at the owner and explains how the pieces meet.
Where it disagrees with an RFC, the RFC wins and this file is the bug.

**Every number lives in a command, not in this file:**

```
python3 scripts/nros-build-wiring.py          # roads, scripts, knob sources, readers
python3 scripts/nros-build-wiring.py --roads  # just the road table
python3 scripts/nros-build-wiring.py --scripts  # the build.rs census, by ROLE
```

That is deliberate. A hand-authored census is the failure mode this repository
has recorded more than any other — the package-directory list that named four
directories which no longer existed while omitting four that did (issue 1211),
the RMW parity map that disagreed with the shape tool by 25 symbols, the
sizes-header family. A table of counts reads as current and answers about the
tree somebody measured once.

## The five stages

`nros build` (RFC-0065 D1) is one pipeline, whatever the road:

```
1. DISCOVER   walk package.xml → pkg index → topological order
2. RESOLVE    the image: argument > [system] default_images > list and fail
3. PREFLIGHT  toolchains / SDKs / sources present?
4. GENERATE   msg bindings + system model + the ROOT BUILD FILE → build/<coord>/
5. EXEC       cmake --build / cargo build / west build — stderr untouched
```

The road is chosen at stage 4/5 **by the board, never by the language mix**
(RFC-0065 D3). A workspace that crosses languages goes to cmake, because cargo
can be consumed as a cmake target through Corrosion and cmake cannot be
consumed as a cargo target (RFC-0024 §6.3). Choosing can also **fail**: one
combination — an esp32 board with a graph that crosses languages — has no road
and says so, rather than naming a tool that cannot build it.

## The three roads, and the thing that actually differs

The stages are shared. What is **not** shared is how a resolved knob reaches
the compiler — and that is the whole of this document, because each road
carries it differently and **each carrier has produced its own delivery
defect**:

The road names below are `Driver` variants (`Cargo`, `CMake`, `West` in
`packages/cli/nros-cli-core/src/builder/plan.rs`); `check-build-wiring-roads`
holds this table and the enum to each other in both directions.

| road | exec | emits a root | carries a knob by | the defect it produced |
| --- | --- | --- | --- | --- |
| **cargo** | `cargo build` | yes | `[env]` rows in the generated `nros-cargo.toml`, read via `--config` | **0491** — a PATH-valued row has three spellings, so a `rerun-if-env-changed` on it rebuilds forever; watch the CONTENT |
| **cmake** | `cmake --build` | yes | `corrosion_set_env_vars()` on the target's own cargo command; the APP rung as a `--config` `[env]` file on the same command (`NanoRosImageEnv.cmake`) | **0460** — `set(ENV{})` touches only the configure process and carries nothing to a cargo lane; **1712** — the image's `[image.<id>] env` had no carrier at all, and `cmake -E env` overwrites an exported variable, so it cannot carry a rung that sits below `env` |
| **west** | `west build -b <board>` | no | `$DOTCONFIG`, read per build script through `nros_zephyr_build` | **0460** — zephyr-lang-rust builds its own cargo command and inherits no environment at all |

One road emits no root on purpose: a Zephyr app is already a complete cmake
project and its Kconfig overlays are user intent, so *stage 4 emits a root only
where a root would otherwise be hand-written* (RFC-0065 D3).

### The road that was deleted, and the test that finds the next one

There was a fourth, `idf.py`, chosen for `platform = "esp32"` whenever the
graph crossed languages. **Its carrier cell was empty** — and that is what
ended it (RFC-0065 D3 amendment, 2026-09-28). Every other road's cell names a
mechanism that has produced a measured defect; an empty one says the road
delivers no resolved knob at all, which is issue 0460's class applied to a
whole road rather than to one lane of one.

So the question this table asks of a new road is not "does the tool work" but
**what carries a knob on it, and how would I know if nothing did**. A road
that cannot answer is not a capability with a missing test; it is a claim.
`nros build` now REFUSES that combination, naming issue 1525, because
answering `cmake` instead would be the same claim in a tool that also has no
esp32 road.

There WAS a fourth carrier that is not a road. On **NuttX**,
`<nros/nros_config_generated.h>` resolved to a **committed snapshot**, not the
per-build header, so a template edit that the generated artifacts consume was
a two-file change (issue 1115) — forgetting it broke every NuttX C and C++ image
from a clean clone for two days — and the snapshot's sizes fell below the build
four times (issue 1568 measured a live overrun). Issue 1569 retired it: the
NuttX FFI build reads the per-build headers from the `nros-c`/`nros-cpp`
`links` channels (`DEP_NROS_{C,CPP}_CONFIG_INCLUDE`), which also order it after
both writers. The snapshot survives as `*_buildless.h` for header-only checks
that compile with no build, behind `NROS_CONFIG_BUILDLESS`, which
`check-config-fallback-macros` forbids any build to define.

## The ladder: what may state a value

RFC-0049 orders the rungs **builtin < platform < board < env**. A higher rung
that states a value wins; a rung that is silent is not a zero.

| rung | stated in | owner |
| --- | --- | --- |
| builtin | the reading crate's own default | the crate |
| platform | `nros-platform.toml` (`config/*/`, `packages/platform/*/`) | RFC-0049 |
| board | `nros-board.toml` `[knobs.<group>]` | RFC-0064 R5 D4, RFC-0103 D6 |
| app | `[image.<id>] env` in the leaf's `system.toml` | RFC-0049 / RFC-0098 |
| env | the process environment | RFC-0049 |

**One name per knob** (RFC-0103 D6, phase-484 W5a). Every knob is one row in
the registry `packages/tooling/nros-zephyr-build/knobs.toml`: id
`<group>.<name>`, env `NROS_<GROUP>_<NAME>` (a knob of one RMW is
`NROS_<RMW>_<X>`), Kconfig symbol `CONFIG_NROS_<ID>` unless the row names
another. Everything else is generated from that file: the Rust `KNOBS` table
and its derived `KCONFIG_PAIRS`, cmake's `NanoRosKnobRegistry.cmake`, and the
knob section of `zephyr/Kconfig`. An older spelling (`ZPICO_MAX_PUBLISHERS`)
is a RETIRED row (phase-484 W8): every reader on every road — a build script,
a configure, the CLI's `[image.<id>] env` — refuses one that is set, in the
registry's one refusal sentence, naming the canonical spelling. The row stays
one release as that error table; `check-retired-knob-spellings` keeps every
env reader and writer off the old names (the same words stay as the zenoh-pico
shim's C macro names).

**The app rung's two spellings, one set of rows** (phase-484 W8, RFC-0103
D6). An image states a registry knob as `[image.<id>.knobs.<group>] <name> =
<value>`, the shape a board's `[knobs.<group>]` has (the key is the row's
`toml` key, else its id split at the first `_`). `cmd::leaf_settings::app_rung`
validates each key against the registry (`knob_by_image_key`: unknown key,
type, `range`, `names` are errors naming it) and renders it to the knob's env
spelling, which is the row an `[image.<id>] env` entry states. Every carrier
below is fed from those rows, so moving a row between the two spellings
changes no byte of any carrier (measured on 29 rows: 575 dumped files
identical, the west `Kconfig.from-system-toml` differing only in its
provenance comment). An `env` row naming a registry knob, by its env name or
by the Kconfig symbol west renders it to, still works and prints one
`IMAGE_ENV_DEPRECATED_TEMPLATE` warning; stating a knob both ways is an error.
The gate `no_tracked_system_toml_states_a_registry_knob_as_an_env_row`
(`check-cli-tests`) keeps the tree on the knobs spelling.

**The app rung's carrier, per road** (issue 1712). Cargo: the generated
`nros-cargo.toml` `[env]`. CMake: `--config <build>/nros/nros-image-env.toml`
on every cargo command the configure spawns (written by `nros ws leaf-system
--image-env-out`), the same unforced `[env]`, so an exported variable still
wins and a changed row re-runs the build scripts that read it. West
(phase-481 W1, RFC-0098 D11, phase-484 W5b): the nano-ros module's
`module_ext_root` hook writes two files. Knobs go to
`<build>/nros/Kconfig.from-system-toml`, which the module's prompt-less knob
section `osource`s first, so the image's `[image.<id>.knobs]` (or `env`)
value is the symbol's `default` and reaches `$DOTCONFIG`. A one-build override is
`-DNROS_OVERRIDE_<ID>=<v>`; a `-DCONFIG_<knob>` or a knob line in a conf file
is refused, naming `system.toml`. The image's RMW choice, endpoint and domain
stay in the conf fragment `<build>/nros/<image>.conf`, LAST in
`EXTRA_CONF_FILE`, because a Kconfig `choice` cannot be a prompt-less default
(W5b's M1). A row with no Kconfig symbol rides the same unforced `--config`
`[env]` file on the Zephyr cargo commands. A Zephyr leaf's conf files hold no
nano-ros knob at all (gate `check-leaf-conf-nros-knobs`). A workspace
image whose configuration differs gets its own cmake configure
(`build/<coord>-cfg<hash>/`, D12).

Beside the ladder sit two things that are **facts, not rungs**, and the
distinction matters because a fact has no `$DOTCONFIG` counterpart:

* **Derived entity facts** — `NROS_DECLARED_*`, `NROS_ENTITY_*`, computed by
  cmake for this image from the resolved SystemModel. Absent means "no answer",
  never "zero" (issue 1122).
* **The sizing descriptor** — `<build>/nros/sizing/<entry>.toml`, per-FIELD
  stated-or-refused (RFC-0100 D4). Read it through `nros-sizing-descriptor`,
  never by hand, and never re-carry one of its facts through an env knob.

**A platform that resolves to no descriptor is an ERROR** (phase-468 W1): every
board-declared name has a descriptor or an explicit declaration that it has no
rungs, and an unanswered name panics rather than warning.

## Where a resource comes from: the location ladder

A build also needs things it did not produce: vendored source trees, cross
toolchains, the `nros` CLI, generated artifacts. Before phase-484 a resource's
location was decided by 13 mechanisms, and 13 resources were found two or more
ways (issue 1767, archived with a per-item table). RFC-0103 D3 gives them one
ladder, `nros_build_paths::locate`, with these rungs, highest first:

| rung | stated by |
| --- | --- |
| `arg` | an explicit argument |
| `env` | the resource's one `env` name in its index row |
| `project` | `[sources]` / `[tools]` in the project's `system.toml` |
| `local-edit` | a checkout submodule moved off its pin, or dirty |
| `store` | `$NROS_HOME/sources/<name>/<version>+<sha8>/`, materialised by `nros setup --source` |
| `checkout` | the submodule in this checkout |

Every road asks the same ladder, never a copy of it:

| road | front end |
| --- | --- |
| a build script | `nros_build_paths::locate::source(name)`; it watches the answer's CONTENT, never the variable (issue 0491) |
| cmake | `nros_locate_source()`, `nros_locate_var()`, `nros_locate_artifact()` in `cmake/NanoRosLocate.cmake`: one `nros locate --all --format cmake` per configure, normal variables, never `CACHE` |
| shell / `just` | `nros_locate_source` / `nros_locate_artifact` in `scripts/build/cargo.sh` |
| Python gates | `scripts/lib/source_locate.py` |
| a person | `nros locate <name> --why` prints the rung that answered |

The image's resolve records the answers in `resolved.toml [locations]` (path,
rung, and `transient` for an environment answer). That file is a record, not
a carrier: build scripts re-ask the ladder.

Two roots, one name each (phase-484 W1/W2d): the store is `$NROS_HOME`
(`NROS_STORE` and `NROS_SDK_STORE` are refused, naming it), and the checkout
is found by `--nano-ros-path` (`-DNROS_REPO_DIR` in cmake), then a walk up
from the workspace, then the `nros` binary's own location (a release's
`share/nano-ros`, or the checkout an in-tree build sits in). No environment
variable names it. `activate.sh` exports no location at
all: it puts `nros` on `PATH` and nothing else, so a linked worktree cannot
inherit another checkout's trees (issue 1280).

The last three resources found more than one way went through the same
fix, each with a gate that refuses a new hand-written path:
- the `nros` and `nros-launch-resolve` binaries: `nros_cli_bin
  [--checkout-only]`, `nros locate tool`, and in Rust
  `nros_cli_in_checkout` / `launch_resolver_from` (issue 1850,
  `check-host-tool-path-literals`);
- QEMU: `nros_qemu_bin` and the `nros-qemu-ladder` crate (CLI + tests; issue 1851,
  `check-qemu-one-ladder`);
- the generated-header directory names: `nros locate artifact` and
  `cmake/NanoRosArtifactLayout.cmake`, which read `nros_build_paths::artifact`
  (issue 1852, `check-artifact-path-literals`).

## Where a selection is stated

The board, RMW, domain, locator, IP address and transport are stated in the
image's `system.toml` (`[image.<id>]`, `[system]`). The board's descriptor
supplies the defaults through `[board.net]`: every QEMU board joins the one
`qemu` network (`10.0.2.0/24`, gateway `10.0.2.2`), and a router default is
derived from the gateway as `tcp/<gateway>:7447` (phase-484 W4b). The board
crate reads generated `BOARD_NET_*` constants rather than restating them.

On the cargo road `nros::main!` bakes the image's selections into a deploy
overlay, and every board family merges it over its own defaults with
`BaseConfig::apply_overlay` before opening the session. That merge is the
only way the image's locator reaches a board, because
`BootConfig::over_board_defaults` lets the board's value win. NuttX once
skipped it and every NuttX Rust image dialled an empty locator (issue 1836),
so `check-image-locator-bake` now checks that each family's crates apply the
overlay rather than trusting that they do. `ROS_DOMAIN_ID` and `NROS_LOCATOR`
remain runtime overrides; `NROS_BOARD`, `ZENOH_LOCATOR` and a build-time
`NROS_DOMAIN_ID` are not inputs.

## Where a value must NOT be stated

Three of these are gated, because each was written by hand at least once:

* **Not in `examples/**/.cargo/`** — an example's build configuration is
  generated from one board choice (RFC-0098 D1). Gate:
  `check-example-cargo-dirs`.
* **Not in a committed SystemModel** — models are build artifacts. Gate:
  `check-no-tracked-models`.
* **Not in a second spelling of a derivation** — one formula, and its INPUTS
  derived once too (issue 1025). Gate: `check-fixture-artifact-dir-inputs`.

## Reading a knob: the two shapes

A Kconfig knob reaching the Zephyr C lane and not the Rust one is issue 0460,
the hazard this whole area exists to contain. Readers come in two shapes and
they are **not interchangeable**:

* **DERIVED** — the Kconfig name is built from the env name as
  `CONFIG_{env_name}`. A knob such a reader names is a knob it resolves, so
  "does the file mention it" is a sound test.
* **TABULATING** — an authored table, needed where the two vocabularies differ
  (`NROS_ZENOH_SUBSCRIBER_RING_DEPTH` ↔ `CONFIG_NROS_SUBSCRIBER_RING_DEPTH`). Here a
  mention proves **nothing**: three knobs were mentioned with no table row and
  silently compiled crate defaults (issue 1490).

Held by `check-kconfig-knob-forwarding`. Both shapes now go through one
resolution function, `nros_zephyr_build::knob("<ENV>")` with the Kconfig and
env sources as inputs (phase-468 W4). Since phase-484 W5a the TABULATING
table is no longer authored: `KCONFIG_PAIRS` is derived from the knob
registry, so a missing row is a missing registry row, which
`check-zephyr-kconfig-knobs` holds against the rendered Kconfig.

### How to prove a knob is delivered

**The baseline cannot show a delivery failure.** Unset, the Kconfig default and
the crate default are usually the same number, so "delivered" and "fell back to
the same value" are one observation. Use a **non-default probe**:

```
# append to the leaf's prj.conf, rebuild, read the generated const, then REVERT
CONFIG_NROS_SUBSCRIBER_RING_DEPTH=7
```

That is how issue 1490 was found and how its fix was accepted (`4` → `7`).

## Who reads a carrier: the build scripts

A carrier delivers to a **build script**, and there are 66 of them. What each is
FOR is a ROLE, printed by `--scripts`; phase-471 replaced a set of capability
LETTERS with those roles because a letter was a grep for a TOOL and a tool is
not a role — `cc::Build` put three first-party test C files in the same class as
the FreeRTOS kernel, and left `cyclonedds-sys` (≈200k lines of vendored C
through `cmake::Config`) reading as "re-roots paths and nothing else".

Two things in that output are load-bearing here and not obvious:

* **`source_origin`** — where a C-compiling script's sources come from. Three
  answers exist: a path-valued SDK variable, a workspace-relative path, and a
  `links=` hand-off (`DEP_*`) from the crate that already resolved the tree.
  **Only the first is exposed to issue 1280**, because only the first can be
  inherited from another checkout. Prefer the `links` hand-off where one is
  available: it also orders the two build scripts, which a shared environment
  variable cannot do.
* **the PATH RESOLUTION section** — issue 1280's BUILD-SCRIPT half, which no
  gate covers (`check-inherited-checkout-paths` holds the shell half and names
  no `build.rs`). It read 5 before issue 1527 and reads 0 now. A build script
  that resolves one of these variables goes through `nros_build_paths`; a
  private helper that takes the name as an argument is the shape that hid two
  of the five, because no literal-matching probe can see inside it.

The classes, the outlier verdicts and the migration order are
[phase-471](../roadmap/archived/phase-471-build-script-classes.md).

## Which RFC owns what

| slice | owner |
| --- | --- |
| the stages, the driver, what stage 4 emits | RFC-0065 |
| the knob ladder and the descriptors | RFC-0049 |
| board organization, `nros-board.toml` as the single source | RFC-0064 |
| a leaf's build configuration is generated | RFC-0098 |
| resolve before configure | RFC-0094 |
| the sizing descriptor | RFC-0100 |
| build caches: one root, one vocabulary | RFC-0070 |
| platform and build determinism | RFC-0042 |
| the backend descriptor | RFC-0071 |
| where a build script gets a vendored tree's ROOT | RFC-0101 |
| one home per fact: the location ladder, knob names, selections | RFC-0103 |

## The recurring failure, stated once

Every defect named above is the same shape at a different site: **a value that
was resolved correctly and delivered to only some of the things that read it.**
0460 (Kconfig reaches C, not Rust), 1115 (the header reaches every road but
NuttX), 0491 (one directory, three spellings), 1025 (one formula, two
derivations of its inputs), 1490 (the table that resolves, missing a row).

So the question to ask of any new wiring is never "is the value right" — it is
**which roads carry it, and how would I know if one did not**. The answer is a
non-default probe on each road, not a reading of the code.
