---
rfc: 0120
title: "`nros configure`: one resolver every road calls, `nros build` as the recommended front door, and single-pass host configures"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-530]
amends: [0094, 0085, 0072, 0117]
supersedes: []
superseded-by: null
---

# RFC-0120 — `nros configure`: one resolver, many builders

**Status:** Draft (2026-11-03). Written from the maintainer discussion of
2026-11-02 and 2026-11-03. That discussion started from what phase-439 left
open: the four host configures RFC-0094 A3 does not cover. It went on to ask
which tool owns the build. Implemented by
[phase-530](../roadmap/phase-530-nros-configure-single-pass.md).

## Summary

Everything `nros` derives before a host build tool runs moves into **one
verb, `nros configure`**. That covers message codegen and the metadata probe
(today `nros sync`, renamed `nros configure` here), plus the census, the resolve phase and the generated build
files (today stages 1 to 4 of `nros build`). Every road calls it:

- `nros build` calls it in-process;
- west and CMake call it from their configure-time hooks (the NuttX
  `apps/external` make road is a guest road: it builds only the runtime
  archive from Kconfig and carries no nros project, so it has nothing to
  configure; a NuttX image built as a nros project takes the CMake road);
- an IDE or a hookless host calls it as a pre-build step.

Dependencies point one way. A host tool may call `nros configure` and never
calls `nros build`, while `nros build` calls the host tool. So:

- `nros configure` is **required** on every road;
- `nros build` is the **recommended** front door, and it configures implicitly
  the way `cargo build` resolves and fetches;
- a user who stays in west, an IDE or twister keeps working.

Because configure runs before every host configure, each host configure
takes **one pass**. The fixed point (`cmake/NanoRosReconfigure.cmake`) is
deleted, which meets RFC-0094 A3 as originally written. Sizing is tight on
every road: a leaf that declares no entities is sized from its **census** by
default, and a host C/C++ compiler is a requirement.

## Problem

### Four host configures still converge by a forged timestamp

RFC-0094 A3, as amended on 2026-10-23, guarantees one pass only for a host
configure that `nros build` started. Four cases are not covered
(RFC-0094 "Not covered"):

1. **A plain `west build` of a leaf.** No resolve runs in front of it. The
   module's early readers see placeholders, and one re-configure delivers the
   derived numbers. Measured on `examples/zephyr/cpp/talker`.
2. **The Rust west entry** (`rust_cargo_application()`). Even under `nros build`
   it has no message-bound seed, and its sizing-descriptor decision is made
   during the configure.
3. **The multi-entry CMake road.** One host configure builds several images
   over one shared runtime (RFC-0100 D12). The per-image resolve does not
   compose the union.
4. **The image-Kconfig stamp** (`zephyr/CMakeLists.txt`, issue 1699). It is
   written after Kconfig, so the pass that notices a `zephyr/Kconfig` change
   has already used the stale `.config`.

The fixed point that converges them cost issues 0940, 0991, 1002 and 1119.
The last of these shipped an executor arena of 70,296 bytes where 46,272 was
correct.

### A plain `west build` is not tightly sized at all

Reading the talker's own `system.toml` shows something the case list hides:

> a standalone Zephyr leaf's entity inventory does not compose (measured,
> phase-481 W3: refused for every C/C++ leaf and five of the six Rust ones)

A leaf can opt in today. If it states `entities = "census"`, the module hook
takes the census before Kconfig (issue 2086,
`cmake/NanoRosLeafCensus.cmake`), and it is sized from that. A leaf that
states nothing, which is every example, gets something else. For it the fixed
point buys tight **message bounds** only, and entity counts fall back to crate
defaults. Issue 2202 measured those defaults
at ~400 KB of RAM on a 520 KiB part, and without a stated knob the image fails
to link. On that road, removing the fixed point and getting tight sizing are
the same work.

### Who owns the build

[RFC-0072](0072-rtos-integration-nano-ros-is-a-guest.md) says the RTOS keeps
its own build tool. [RFC-0085](0085-zephyr-workspace-and-west-handoff.md) D2
promises that `west build -b <board> <app>` works. `nros build` hands off to
the same tools, and run, flash and monitor go through runner packages that
call per-platform tools ([RFC-0118](0118-target-execution-model.md),
[RFC-0119](0119-cli-extension-packages.md)). Read literally, "`nros build` is
the required path" puts a cycle into that picture: `nros build` → west → module
hook → `nros build`. RFC-0085 D2 already forbids the last arrow. So the
question this RFC answers is which verb a host tool may depend on.

## Design

### D1 — Three layers, one direction

| layer | what | depends on | required |
| --- | --- | --- | --- |
| 0 | `nros configure` | declarations, the CLI, a host compiler | **yes, every road** |
| 1 | host tools: west, CMake, make, cargo, IDE builds | layer 0, through a hook or a pre-build step | the user's choice |
| 2 | `nros build`, `nros run`, `nros flash` | layers 0 and 1 | recommended |

Rules:

- **A layer-1 tool never invokes layer 2.**
- **Layer 0 never invokes a target build tool, and never invokes layer 2.**
  It may build a HOST image, as the census and the metadata probe do, by
  calling the host tool (CMake or cargo) directly. Neither enters
  `find_package(Zephyr)`, and the census host build sets
  `NROS_LEAF_CENSUS_HOST`, which every hook honours. This replaces RFC-0085
  D14's "a query cannot loop" with the property that actually prevents the
  loop (D9).
- **The rule is broken today in one place.** The workspace census takes itself
  by spawning `nros build <native image>` (`entity_census.rs:546-560`, reached
  from `build.rs:907-922`). That is layer 0 calling layer 2. Phase-530 W2
  replaces it with a direct host-tool build of the native image.

"Main path" is a documentation decision, not a dependency. The book leads with
`nros new` → `nros build` → `nros run`. No tool requires `nros build`.

### D2 — `nros configure` absorbs `nros sync` and stages 1 to 4

| phase | was | does |
| --- | --- | --- |
| C1 sync | `nros sync` (now `nros configure`'s first phase) | message codegen, metadata probe (cached per component, keyed on its sources), models, leaf lock seed, the interface search path |
| C2 discover | stages 1, 1b, 2 | packages in dependency order, selection, images |
| C3 preflight | stage 3 | toolchains, board provisioning, host compiler (D4) |
| C4 census | stage 3.4 | host build plus counting run; **the default for a leaf with no declared entities** (D4) |
| C5 resolve | stage 3.5 | entity inventory, message bounds, the RFC-0103 knob ladder, sizing descriptor. Per image, or as the union over every image sharing one host configure (D6) |
| C6 generate | stage 4 | entry package, `nros-cargo.toml`, west app, Kconfig defaults, seeds, the leaf `.cargo/config.toml` include |

Contract:

- **One output root, per project, keyed by image.** The project is the leaf
  directory or the workspace root, the same root `nros build` uses.
  - Configure writes `<project>/build/nros/configured/<image>/` with a stamp
    holding the digest of every input that image read.
  - A shared host configure (D6 case 3) gets one stamp per host-configure
    coordinate, covering the union of its images.
  - The root does not depend on any host tool's build directory, so a hand
    run, `nros build` and a hook all find the same stamp. The hook locates it
    from the image identity through `nros locate`. A path passed in by
    `NROS_RESOLVED_DIR` is no longer needed, and that variable retires.
- **What one run covers.**
  - A hand run, `nros build` and `nros run` configure every image the
    selection names, the same set `nros build` builds today.
  - A hook configures only the image its host configure is building: the
    `-DNROS_IMAGE` id, else `default_images`, else the image owning a
    workspace entry (RFC-0085 D14's walk-up).
- **Concurrent runs are serialised.** A per-project file lock is held while
  writing shared state (`generated/`, models, the lock seed, the census). The
  second caller then finds the stamp current. A driver that already
  configured (a fixture prepass, `nros build`) is simply a current stamp to
  every hook it starts.
- **The environment travels with the stamp.** The stamp records the ament
  prefix and the `nros-launch-resolve` path the run used. A hook in an
  IDE-launched host configure, with no sourced shell, behaves like this:
  - stamp current → needs neither;
  - stamp stale → reuses the recorded ones;
  - no record at all → refuses, naming the setup step.
- **A run with nothing changed is a no-op** that costs a digest comparison.
  This is what lets every road call it on every build.
- **It never moves a lock pin.** Configure may SEED a missing leaf lock, the
  way `cargo build` creates one. Moving a pin stays `nros update`, under the
  CLAUDE.md lockfile rule.
- **It computes the interface search path itself.** That retires the
  documented `eval "$(nros dev ws env)"` step.
- **`nros sync` becomes an alias** of `nros configure`, and is then
  **retired** (maintainer decision, 2026-11-03). This is a deliberate exception
  to RFC-0117's "aliases instead of removals". A permanent `sync` would keep two
  names for one verb in every chapter, script and answer a user finds. The
  alias exists only while users and scripts move over. Retirement follows the
  knob-spelling precedent (phase-484 W8), in three steps:
  1. **Alias.** The alias warns on every use.
  2. **Refusal.** `nros sync` exits non-zero naming `nros configure`.
  3. **Removal.** The verb is gone.

  Phase-530 W9 carries it.

RFC-0040 rejected an automatic sync at `nros new` time, because the
environment may not be ready at scaffold time. That reason does not reach a
build, which needs the generated code anyway. `nros build` already detects an
unsynced tree and refuses (`… Run nros sync`); `nros configure` replaces that
refusal under this RFC, run by `nros build` itself.

### D3 — `nros build` and `nros run` behave like cargo

- `nros build` = `nros configure` (if the stamp is stale) + the hand-off to the
  host tool.
- `nros run` = `nros build` (if stale) + the runner.

Nobody has to type `nros configure` on the main path. Typing it by hand is for
the cases `cargo fetch` and `cargo metadata` serve: before opening an IDE,
offline preparation, and CI cache keys.

The hand-off stays in `nros build`, not in a runner package (maintainer
delegation, 2026-11-03). Whether build hand-off should move into RFC-0119
runner packages is left open.

### D4 — Tight, or explicitly loose

A leaf that declares no `[[component]] entities` is counted automatically, by
default, on every road:

- **A C/C++ leaf** is sized from its **census**: its own sources built for the
  host and run in counting mode (issues 2043, 2086). Today that is opt-in,
  `entities = "census"`. That spelling stays valid and now means the same as
  stating nothing.
- **A Rust leaf** is sized from the **metadata probe**: a harness crate run on
  the host, in C1, the producer `west-leaf-sizing` already reads. The leaf
  census does not accept Rust (`entity_census.rs:833-840`), and does not need
  to.

Phase-530 W2 measures why phase-481 W3 saw five of six Rust leaves refused
before it claims the Rust half.

The census's own host build is **sizing-insensitive**. It counts
registrations, so it configures with crate defaults, reads no seeds and runs
no promise checks. That makes it one pass by construction, where today it
builds up to three times (`entity_census.rs:376-387`, issue 1252).

Consequences:

- **A host toolchain is a requirement on every road**, including a plain
  `west build` for real hardware (maintainer decision, 2026-11-03). Concretely
  that is a host C/C++ compiler, plus the host Rust target and the
  store-provisioned Corrosion. nano-ros already needs those two for its own
  runtime. C3 checks all three and names the missing one.
- **A leaf whose sources do not compile for the host** (a component calling
  Zephyr APIs directly, say) is refused. The refusal names two fixes:
  `entities = [...]` (tight), or `entities = "defaults"` (loose, written down).
  Nothing is silently loose.
- **Declared `entities = [...]` still wins** and needs no census.
- **A host image keeps the hosted defaults.** The default reaches a leaf
  whose image is CROSS (its board's platform is not the host's), which is the
  line `nros build` already draws for a workspace: a host image is the census
  producer, never its consumer. A leaf with no `[[component]]` has no program
  for a census to be of and stays unstated. Written `entities = "census"`
  still applies anywhere (phase-530 W2).

### D5 — Every host configure takes one pass

A hooked host calls `nros configure` before its first reader:

| host | where it calls configure |
| --- | --- |
| Zephyr west | `nros_image_kconfig_hook`, which runs from `zephyr_module.cmake` before Kconfig |
| CMake (`find_package(nano_ros)`, `add_subdirectory`) | the top of the package config, before any knob reader |
| NuttX `apps/external` | none: a GUEST road. Its make build compiles only the runtime archive, from Kconfig (`CONFIG_NROS_RMW_*`, `CONFIG_NROS_ROS_EDITION`, `CONFIG_NROS_*_API`), and carries no nros project (no `system.toml`, no image; the app is not an input), so layer 0 has nothing to configure and the archive is sized by its Kconfig knobs. A NuttX image built as a nros project uses the CMake leaf road, which is hooked. Making a make app a project needs a project identity and a knob channel ([issue 2236](../issues/2236-nuttx-make-app-as-project.md)) |

The early readers then READ the seeds. The late producers (`nros_find_interfaces`,
the node-register composer, `nano_ros_entry`, the sizing descriptor) still
recompose, because they see what the host configure actually declared. They
hold that result to the seed with a **promise check**, as RFC-0094 W8 already
does for message bounds:

- equal → continue;
- different over the same inputs → FATAL, naming both files, never re-converged;
- different because the host configure declared something configure could not
  see (a `find_package(<msg>)` not in `package.xml`, a registration the census
  did not count) → FATAL, naming the declaration to add.

No reader ever sees a placeholder that a later pass would replace. So
`nros_reconfigure_settle`, `nros_reconfigure_on_change` and the future-mtime
lever have no callers, and `NanoRosReconfigure.cmake` is deleted. With it go
the W8 pass bookkeeping in `NanoRosResolved.cmake`
(`_nros_reconfigure_pass_props`), which the promise check no longer needs.

If configure already ran (for example, `nros build` handed off to west), the
hook finds the stamp current and returns.

**A promise is owed to a READER.** Only a call that consumes a fragment at
configure time records what it read: the Zephyr knob resolver's loaders, and
on the CMake roads the entity-facts flush that hands the entity counts to
cargo. `nros_find_interfaces()` records no read of its own: it READS the
entity fragment (the payload-class join composes over it), but its output has
a configure-time consumer only on Zephyr, where the loaders already registered
both fragments; on the CMake roads nothing reads the message-bound fragment at
configure time (the payload classes reach cargo through the sizing
descriptor). A producer whose answer no reader consumed is free to
compose it -- unless the fragment was SEEDED: a seed is a promise in its own
right, so recording one snapshots it and registers the end check, and a
composer that moves a seeded fragment fails the configure whatever order the
readers ran in.

**A read with no seed** (a guest application, a host with no CLI, a configure
whose own `nros configure` failed) is held to the same end-of-configure
comparison, and is never re-converged either:

- equal → continue;
- the reader read nothing or a placeholder (which says so on a line of its
  own) → a WARNING: it used its defaults (Kconfig and crate values), which can
  be smaller than this image needs; FATAL when the composed fragment's
  TOP-LEVEL status (`NROS_ENTITY_INVENTORY_STATUS` /
  `NROS_MESSAGE_BOUNDS_STATUS`) is `derived`, since a real answer existed and
  the reader did not get it. A derived sub-field under a refused top level is
  not a derived answer;
- the reader read a derived answer an earlier configure left, and this
  configure composed another → FATAL, because that answer can be smaller than
  this one. The fragment now holds the new answer, so the next configure
  reads it.

Each of these names the road's own remedy: `nros configure ...` where the CLI
drives the road (`nros_configured_locate` resolved a CLI), and otherwise -- a
guest application, a host with no CLI -- re-running this build's configure
(`cmake <build-dir>`, or `west build -d <build-dir>` on Zephyr).

### D6 — The four cases, closed

| case | closed by |
| --- | --- |
| 1 plain `west build` | D5's hook call: C4 census plus C5 resolve, then promise checks |
| 2 Rust west entry | C5 writes the message-bound seed and makes the sizing-descriptor decision for a Rust entry, as it does for C/C++. The derived bounds reach the Rust lane's cargo environment |
| 3 multi-entry CMake | C5 resolves once per **host-configure coordinate** (`cmake_coordinate`, phase-481 W4) as the union over its images. The cmake take (`nros_resolved_entity_take`) accepts N models |
| 4 Kconfig stamp | written in the pre-Kconfig hook, so Zephyr's `.cmake.dotconfig.checksum` moves in the same pass and `.config` is regenerated. It also hashes `Kconfig.from-system-toml` |

A fifth configure-time producer was found while mapping these: the Cyclone
C++ facts compiled at configure time (issue 1653,
`NanoRosSizingDescriptor.cmake`). On the Zephyr road the module read
`west-cargo.toml`'s content inside `find_package(Zephyr)`, before the entry
copied the descriptor there, so the C++ half compiled from the previous
configure's copy. It is closed by ordering, not by a seed: the module's read
is deferred to the end of the top-level configure, after every entry, which is
where the CMake road already applies it (phase-530 W7). A read after its
producer needs neither a seed nor a promise.

### D7 — Freshness: every input reaches the next build

| input that changed | how the next build sees it |
| --- | --- |
| `system.toml`, launch, `package.xml`, `*.msg`, board descriptor, `knobs.toml`, the CLI | a configure input, also registered as a host-configure dependency; one pass |
| the ament install | the content stamps of issue 1780, inside C1 |
| `zephyr/Kconfig` | D6 case 4 |
| a component source | **the census step**, below |

The census step is a host-build step whose inputs are the component's
sources. It writes the census digest write-if-changed (`restat`), and that
digest is a host-configure dependency. Ninja brings a manifest's dependencies
up to date before deciding to regenerate it. So:

- an edit that does not change the entities costs a cached host compile;
- an edit that adds an endpoint costs exactly one host configure, which is a
  real input change and needs no forged timestamp.

This needs a spike first (phase-530 W0): does CMake accept a configure
dependency that is also the output of a build edge, and does ninja order it
that way? If not, the fallback delivers entity-derived knobs at BUILD time:
a generated header, plus a cargo `--config` file with ordinary ninja edges.
That needs no re-configure, and is a larger change.

The fallback cannot reach a value that lives in `.config`. W0 therefore also
lists which entity-derived values are Kconfig symbols. Any such value keeps
one real host configure on an entity-changing edit, which is a genuine input
change, never a forged timestamp.

### D8 — Adapters, and the IDE routes they open

| adapter | calls `nros configure` | hosts |
| --- | --- | --- |
| driven | in-process | `nros build` |
| hooked | in the host's configure (D5) | west (and everything that runs it: nRF Connect for VS Code, the Zephyr IDE extension, CLion, twister, sysbuild), CMake (presets, VS Code CMake Tools, CLion; a NuttX image built as a nros project) |
| hookless | a pre-build step the host already has | PX4, PlatformIO, Eclipse-based IDEs (STM32CubeIDE, MCUXpresso IDE, S32 Design Studio) |
| none | once, then a drop-in | IAR, Keil (RFC-0072's emitter, not shipped) |

An IDE needs more than a build:

- `compile_commands.json` for C/C++ navigation (a CMake output already);
- the leaf `.cargo/config.toml` include, so rust-analyzer reads what `--config`
  gives cargo;
- a debug configuration derived from `[board.run]` and `[board.flash]`
  (RFC-0118).

Phase-530 delivers the driven and hooked rows. The hookless IDE pre-build
emitter, the rust-analyzer include, debug configurations and the drop-in are
left for a later phase.

A workspace entry built by a plain `west build` keeps working: the module
walks up to the package holding `system.toml` (RFC-0085 D14), and configure
resolves the image that owns the entry. A guest application (no `system.toml`)
is unchanged: Kconfig stays its mechanism (RFC-0085 D14, "refined").

### D9 — Vocabulary

- **configure**, unqualified, in nano-ros text means `nros configure`.
- **host configure** means CMake's, west's or make's own configure step.
- RFC-0094's text, the phase docs and the book use the two terms this way.

`nros configure` borrows the autotools sense of the word: check the
environment, decide the options, write generated configuration for a build
tool. It does not generate the host's build system; the host does that in its
own configure.

## Amendments to other RFCs

- **RFC-0094 A3.** The amended text ("a configure `nros build` started takes
  one pass") is superseded by the original ("the fixed point is gone"). This
  RFC is how A3 is met on every road.
- **RFC-0085 D2 and D14.** The supplier D2 left unnamed is `nros configure`.
  D14's supplier "produces no artifacts" and argued that a query cannot loop.
  The supplier now writes, and the loop is prevented by D1's rule that layer 0
  never invokes a target build tool. `nros dev image facts` stays as the
  read-only query it is.
- **RFC-0072.** "Your RTOS keeps its own build tool" is the rule for an
  existing project adopting nano-ros. A project started with `nros new` builds
  through `nros build`, the recommended path (D1).
- **RFC-0117.** `configure` joins the user tier's Build group, and `sync`
  becomes its alias.

## Alternatives considered

- **Make west a deployment-only tool, with `nros build` the only builder.**
  This saves only case 1. Cases 2 and 3 are gaps inside `nros build`, and case
  4 fires in a build directory `nros build` created. It would break every tool
  that runs west or CMake itself (IDE extensions, twister, sysbuild), and
  RFC-0085 D2's promise.
- **Keep the fixed point for configures `nros build` did not start**, as
  RFC-0094 A3 was amended. It leaves a plain `west build` loosely sized
  (Problem) and keeps the mechanism behind four issues.
- **An undeclared leaf gets defaults plus a hint.** Issue 2202's numbers say
  defaults do not fit a small part. The maintainer chose tight on every road.
- **Calling the metadata probe from the hook.** The probe is a host CMake
  project that cannot configure a leaf whose CMakeLists needs
  `find_package(Zephyr)`. The census can.
- **The name `nros prepare`.** It describes the same thing, but `configure`
  is the word users of autotools and CMake already read correctly (D9).

## Open questions

1. Does the census step's digest work as a host-configure dependency (D7)?
   Phase-530 W0 answers it.
2. Should build hand-off move into RFC-0119 runner packages, so build, run and
   flash share one mechanism?

(Open question 3, where the ament install is recorded for an IDE-launched
configure, is answered by D2: the stamp records it.)

## Changelog

- 2026-11 — created.
- 2026-11 — D4: the census default is for a CROSS image with at least one
  `[[component]]`; a host image keeps the hosted defaults (phase-530 W2).
- 2026-11 — Summary, D5, D8: the NuttX `apps/external` make road is a GUEST
  road sized by its Kconfig knobs, not a hooked host: it builds only the
  runtime archive and carries no nros project, so there is nothing to
  configure (phase-530 W5, Ruling R36; issue 2236 tracks making a make app a
  project). A NuttX image built as a nros project takes the hooked CMake leaf
  road.
- 2026-11 — D5, D6: a promise is owed to a reader, and a seed is held to itself
  whatever the reader order; an unseeded read is held to the same end check
  (defaults WARN, FATAL over a derived answer; a moved earlier answer is
  FATAL); the
  fifth producer (Cyclone C++ on west) is closed by deferring its read past
  its producer. `NanoRosReconfigure.cmake` is deleted (phase-530 W7).
