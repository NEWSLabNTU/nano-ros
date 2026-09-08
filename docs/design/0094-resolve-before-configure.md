# RFC-0094 — Resolve before configure: the declared thing is the thing that runs

**Status:** Draft (2026-09-08)

Supersedes nothing. Amends RFC-0065 (D1's stage list gains a stage), RFC-0071
(D5's index becomes load-bearing) and RFC-0087 (D2's `build_type` gains a
reader). Depends on neither being finished first.

## The defect this exists to remove

Four independent studies of the build system, run in parallel against
`origin/main` on 2026-09-07, each traced a different seam and each landed on the
same shape. Stated once:

> **A declaration exists, is authored, is gated for correctness — and something
> else is load-bearing.**

| declared | what actually runs | consequence |
| --- | --- | --- |
| `<nano_ros_provides kind="rmw">` + `nros-rmw.toml` | `RMW_ROWS`, a table baked into the `nros` binary at CLI-compile time | a provider is discoverable and not dispatchable |
| `<build_type>`, 411 tracked declarations | `CMakeLists.txt` presence, tested at three sites | `colcon` routes on the declaration; `nros build` does not |
| `nros-platform.toml` | two separate directory walks | 5 of 8 descriptors have no `package.xml`; the gate globs one root of two |
| "core is Rust for safety" | five disagreeing definitions | one names a crate that does not exist |

None of these is a failing gate. Every one is green. In each case a second
mechanism grew beside the declared one and the declared one never became
load-bearing.

The measured cost is not theoretical. Knob values are decided *during* configure,
which forces a three-pass fixed point implemented with a forged future mtime
(`zephyr/cmake/nros_cargo_build.cmake`), and that machinery cost issues 0940,
0991, 1002 and 1119 — the last shipping an executor arena of 70,296 bytes where
46,272 was correct.

## The rule

**One place decides a knob. Every other place reads it.**

Concretely: `nros build` gains a resolve phase between "is the toolchain
present" (stage 3) and "emit a root build file" (stage 4). It reads
declarations, never compiled artifacts, and writes one file. Stages 4 and 5 may
read that file and may not re-derive its contents.

## Why a phase and not more forwarding

The alternative — keep deriving during configure and forward harder — has been
tried and is what the three-pass fixed point IS. Each round of forwarding closed
one delivery gap and left the next (`check-knob-delivery.py`'s docblock records
four delivery failures in one wave with every existing gate green).

The direction problem is structural, not a plumbing shortfall.
`nros-rmw-zenoh` is a **dependency** of the leaf, so the crate that must know
the entity counts compiles *before* the crate whose source declares them. No
build script, proc macro or manifest key can reach backwards across that edge —
which is why issue 0827's fix moved the derivation OUT of the cargo graph into
`nros sync` in the first place. This RFC finishes that move rather than starting
a new one.

## Prior art, checked on this host

**ESP-IDF v5.3.0** does exactly this and is the closest fit. `component.cmake`
runs a separate `cmake -P` with `idf_component_register` **redefined as a stub**
that captures only `REQUIRES`/`PRIV_REQUIRES` and returns, under
`CMAKE_BUILD_EARLY_EXPANSION=1`. The complete component graph exists before any
real configure, with no compile. Values then travel through a property bag
(`idf_component_get_property`) with a generator-expression mode for values not
yet known at read time.

**Zephyr v3.7.0** derives BOTH representations from one pass: `kconfig.py`
writes `.config` and `autoconf.h` in one call; `import_kconfig()` re-reads the
same file into CMake variables. nano-ros does this on one lane already
(`NROS_RESOLVED_*`). What cannot be copied is `-imacros`: it reaches C and not
rustc, which is issue 0460 and is why the cargo lane needs its own carrier.

**NuttX** is the only one of the three with genuinely independent per-package
compilation (`libapps.a`, each app `ar`-ing under `flock`, registration by the
make `REGISTER` macro writing `registry/*.bdat` concatenated into
`builtin_list.h`). It can do this because every app is C, its interface is a
`{name, priority, stacksize, &entry}` row, and **the `.config` that sized
everything ships with the export**. That last clause is the one nano-ros cannot
satisfy today — see "Out of scope".

## Design

### D1 — Stage 3.5, the resolve phase

Between stage 3 (preflight) and stage 4 (emit root):

    reads   nros-{rmw,board,platform,serdes}.toml   (the descriptors)
            [package.metadata.nros.component] entities
            the contract sidecar, via `EntityInventory::from_model`

**Correction, from phase-439 W2.** An earlier draft named
`nano_ros_node_register(… ENTITIES …)` as the cmake-side declaration. That
argument was RETIRED by phase-412 and is now a `FATAL_ERROR`
(`cmake/NanoRosNodeRegister.cmake:1229`); it stays PARSED only so the refusal can
name it. The live declarative source is the contract sidecar folded into the
SystemModel, which is what stage 3.5 reads — still no compiled artifact, which is
the property that matters.
    runs    EntityInventory::derive                  ONCE, for the whole image
    writes  build/<image>/resolved.toml

No compile. No configure. No cargo invocation.

### D2 — `resolved.toml` is the single answer, and it carries its own provenance

```toml
[image]
entry = "talker"   board = "mps2-an385-freertos"   rmw = "zenoh"
digest = "a3f1c9e2"

[executor]
max_cbs = 1   max_sc = 8   max_nodes = 4   arena_size = 8192

[zpico]
max_publishers = 1   max_subscribers = 1
max_queryables = 8   max_large_subscribers = 0

[provenance]
max_cbs        = "src/talker/package.xml → 1 publisher, 1 timer"
max_queryables = "INFRA: param_services 6 + lifecycle 5 → floor 8"
```

`[provenance]` is normative, not decoration. Today "which number did this image
build at?" is answered by cache-variable archaeology.

`digest` hashes every resolved value. See D4.

### D3 — `build_type` selects the DRIVER; file presence selects PARTICIPATION

These are two questions and conflating them is a defect in BOTH directions.

Measured over 416 tracked `package.xml` (411 of which carry a `<build_type>`;
five declare none):

    both CMakeLists.txt + Cargo.toml :  21
    CMakeLists.txt only              : 173
    Cargo.toml only                  : 158
    neither                          :  64

The 21 are almost all Zephyr Rust leaves declaring `nros_cmake` — correctly, as
cmake drives and the `Cargo.toml` is an implementation detail inside that build.
Today `builder/cargo_root.rs` pulls them into the generated `[workspace]
members` on file presence alone.

**W0 measured that 20 of the 21 are already excluded by a different mechanism** —
13 declare their own `[workspace]` (cargo REFUSES such a member: `multiple
workspace roots found in the same workspace`) and 13 declare
`[package.metadata.nros.entry] deploy`, which `cargo_excluded_entry_dirs`
resolves through the board catalog to `Driver::West`; six carry both. So for
those D3 changes nothing observable — it reaches today's answer from the
declaration instead of a `Cargo.toml` metadata round-trip.

The one genuine repair is `examples/workspaces/mixed/src/rust_heartbeat_pkg`: a
cmake-driven Rust node carrying its own `[workspace]`, which makes the generated
cargo root unusable the moment `mixed` routes to the cargo driver. This RFC did
not name it; W0 found it.

**All 64** declare-but-no-file packages would be hard-failed by routing on the
declaration alone — not the 20 an earlier draft of this RFC claimed, which came
from reading the head of a frequency table rather than the whole of it. The real
breakdown, measured by phase-439 W0:

    23  nros_cargo      22  nros_cmake      12  ament_cmake
     2  ament_cargo      5  (no declaration)

And they are not an interface-package accident: **34 are bringups, 13 are
platform/board descriptors, 17 are interface/message packages** whose build
files are generated at sync time. The case for keeping participation on file
presence is three times stronger than this RFC first stated.

So:

    build_type     → which driver, IF this package is built here
    file presence  → IS it built here
    the gate       → a PARTICIPATING package must have the files its type needs

### D4 — Every cargo-directory key carries the resolve digest

`nros_share_corrosion_cargo_dir(KEY …)` on the `nros-c` lane keys on
`features, rmw, board, caps, profile, target` and **no knob values**. The Zephyr
lane already includes every `NROS_RESOLVED_*`.

D1 makes per-image knob divergence the normal case rather than the exception, so
without this two images differing only in a derived knob share one cargo
directory and the second silently gets the first's numbers — issue 0616's shape.

**This is a precondition, not a follow-up.** D1 without D4 is worse than the
status quo.

### D5 — Descriptors become load-bearing; the closed lists go

**LANDED, phase-439 W4 (2026-09-08).** `nros_rmw_dispatch()` asks
`nros ws rmw-dispatch <name> --lines`, which resolves over the provider scan;
the generated chain, its generator and the root's hand-written chain are all
deleted, and the root dispatches on the DECLARED link strategy (`umbrella` /
`cmake`) instead of on names. `NANO_ROS_RMW=uorb` configures. Issues 1214,
1215 and 1216 are closed; 1219's gate is not written and the issue records the
survey that says why.

`cmake/NanoRosRmwDispatch.cmake` is GENERATED from `rmw_resolver.rs`'s
`KNOWN_RMW` and re-emits, as an `if/elseif` chain, the same eight values that
already live in each backend's `nros-rmw.toml`. Its `else()` arm is a
`FATAL_ERROR` naming a closed set.

cmake asks instead of being generated at — the pattern `NanoRosProviders.cmake`
already uses ("cmake never parses the index. It asks the CLI for a shape it can
read"). An unknown name becomes "no provider announced this", answerable by the
index that already resolves an out-of-tree `rmw:acme` correctly today.

There are TWO closed lists. The generated one has a single upstream source and
dies with this change; the hand-written one at the root `CMakeLists.txt` is a
separate, smaller edit. **`uorb` — an in-tree, zero-Rust C++ backend — is absent
from both**, so the enum has already stopped covering the tree it governs.

### D6 — Configure-time queries declare the tool as a configure dependency

`execute_process()` has already run by the time ninja decides anything, so the
freshness of anything it emits reduces to *does a configure happen* (issue
1018). Every new query added by D5 goes through the existing
`nros_codegen_tool_reconfigure()`, which appends the tool to
`CMAKE_CONFIGURE_DEPENDS`.

## Acceptance

The first deliverable is a test, not a feature.

**A1 — the routing diff.** Push all 411 packages through D3's rule and diff the
resulting member/subdirectory lists against today's. Every package that changes
side is named, and classified as a fix or a regression, before any code lands.
No build required.

**A2 — a fifth backend costs zero core edits.** Add a provider announcing
`kind="rmw" name="acme"` with a descriptor, a `CMakeLists.txt` and a C
`nros_rmw_acme_register`. It must configure and link without editing
`NanoRosRmwDispatch.cmake`, the root `CMakeLists.txt`, or recompiling the `nros`
binary.

*Met, phase-439 W4, with `uorb` as the in-tree proof: it announces itself, ships
a descriptor and a `CMakeLists.txt`, has no `Cargo.toml`, and was refused by both
closed lists. `NANO_ROS_RMW=uorb` now configures and `add_subdirectory` creates
its target. A uorb IMAGE still does not link on a plain host — `orb_*` is PX4's
uORB middleware, which is a property of that backend and not of the seam.*

**A3 — the fixed point is gone.** No lane re-derives a knob during configure;
`nros_reconfigure_settle` and the future-mtime arm are deleted, and Zephyr
converges in one pass.

**A4 — two images differing only in a knob do not share a cargo directory.**
Mutation-tested: revert D4 and the collision reappears.

## Out of scope

**Per-package independent builds.** The tree has already run this experiment:
148 of 160 workspace roots are their own root today, producing 100 GB of cache
and `nros-core` compiled eight times with eight distinct `-C metadata`
identities at one feature set. The C/C++ half could work — NuttX proves the
shape, and it carries 151.7 of 182.3 GiB — but it needs an artifact identity
stamp (rustc version, triple, features, and every knob value the archive was
compiled at) so a mismatched link REFUSES rather than surfacing as a
wrong-sized `_opaque` at runtime. That stamp does not exist, and D1 is its
precondition.

**The `std` deletion, the unsafe census, and workspace membership.** Measured by
the same studies and filed separately (1208–1221); none blocks this.

## Issues this closes or unblocks

Closes: 1207 (build_type unread), 1214 (discoverable not dispatchable), 1215
(root closed list), 1216 (unconsumed dispatch outputs), 1219 (no rmw-agnostic
gate).

Unblocks: 1197 (the FreeRTOS heap pairing — the board reads `resolved.toml`
rather than needing a channel into `nros-node`'s dependency graph), 1171, 1198.
