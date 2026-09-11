---
id: 1219
title: "RFC-0071's `check-rmw-agnostic` gate was never written, so the closed backend lists grew from three to at least five while the RFC's other waves landed (phase-439 W4 removed three of the five; the GATE is still unwritten)"
status: resolved
area: ci, rmw, api
severity: medium
found: 2026-09-08
related: [1214, 1215, 1216, 1218, RFC-0071, RFC-0094, phase-439]
---

# The verification section is the part that did not land

RFC-0071's Verification section is explicit that the RFC is falsifiable by grep
and should be gated that way:

> `packages/core/**` and `packages/api/nros{,-c,-cpp}/**` contain no
> `zenoh|xrce|cyclonedds|uorb` outside prose/comments — a `check-rmw-agnostic`
> gate in the spirit of `check-ffi-struct-mirrors`.

**No such gate exists.** `just/check/rmw.just` has 19 recipes and none is
`rmw-agnostic`; nothing under `scripts/` greps core or api for backend names.

The nearest relative is worse than absent. `scripts/check-decoupling.sh` is
wired at `just/check/lanes.just:985` under `[group("debug")]`, and its own header
says:

> SUPERSEDED (2026-06-09) — RFC-0031 … deliberately RESTORED the `?/` forwarding
> + optional backend deps in the `nros` umbrella … So this guard now tests a goal
> [that is no longer the goal]

— and it is documented as expected-to-fail and un-wired from `just check`. Its
coverage was narrower than the rule anyway: it matched only Cargo
`[dependencies]` / `dep:` / `?/` lines in two manifests, never source, never
CMake. This is the issue-0196 class (a gate whose coverage is narrower than the
rule it enforces), plus a gate that is now measuring the wrong thing.

## What grew in the gap

RFC-0071 measured **three** disagreeing closed lists of backend names.
phase-347 W3 reconciled those three onto `NROS_RMW_KNOWN`
(`cmake/NanoRosFeatureSet.cmake:122`, `packages/api/nros-cpp/CMakeLists.txt:271`,
`cmake/NanoRosRmwDispatch.cmake:58`). The count today is **at least five**, and
the two the RFC never inventoried both omit `uorb` — the same failure the RFC
used as its motivating evidence:

| site | shape | `uorb`? |
| --- | --- | --- |
| `CMakeLists.txt:267-402` | `if/elseif/else` → `FATAL_ERROR` | **absent, fatal** (issue 1215) |
| `packages/api/nros-c/cmake/NanoRosLink.cmake:148-162` | `_nano_ros_rmw_targets` if/elseif | **absent, silent** (issue 1218, dead file) |
| `packages/cli/cargo-nano-ros/src/workspace_scaffold.rs:304-337` | `"cyclonedds" \| "zenoh" \| "xrce" => {}` else `bail!` | **absent, rejected** |
| `packages/cli/colcon-cargo-ros2/colcon_nano_ros/task/nros/build.py:63` | `RMW_BACKENDS = ("zenoh", "xrce", "cyclonedds")` | **absent** |
| `packages/core/nros-macros/src/main_macro.rs:2612-2620` | `fn rmw_crate_ident` closed `match` | **absent** |

## The core leaks the RFC says should not exist

The RFC's claim about `packages/core/**` is *substantially* true — a
name-count census finds 421 hits of which 378 are migration commentary — but not
literally true. The production, branching sites:

* `packages/core/nros-macros/src/main_macro.rs:2612-2620` — a closed `match` on
  `"zenoh" | "cyclonedds" | "xrce"` mapping a name to a crate identifier, in a
  **core proc-macro crate**. Its own doc comment records that it is the second
  spelling of the same table, the other being
  `packages/cli/nros-cli-core/src/orchestration/bridge_gen.rs:367-375`.
* `packages/core/nros-macros/src/main_macro.rs:2508-2510` — filters on
  `rmw == "cyclonedds"` and emits a hardcoded
  `::nros_rmw_cyclonedds::register::<#path>()`.
* `packages/core/nros-orchestration-ir/src/cyclonedds_type_sizing.rs` (267 lines,
  registered at `lib.rs:38`) — an entire core module named after one backend,
  hand-mirroring that backend's `MAX_TYPES` default.

(`packages/core/nros-serdes/src/format.rs:39-82`'s `Uorb` is the *serialization*
axis per RFC-0088, not the RMW axis; counted, not indicted.)

## The api leaks are unchanged since the RFC described them

`packages/api/nros-c/Cargo.toml:84-88` still reads:

```toml
rmw-zenoh      = ["rmw-cffi", "dep:nros-rmw-zenoh"]
rmw-xrce       = ["rmw-cffi", "dep:nros-rmw-xrce-cffi"]
rmw-cyclonedds = ["rmw-cffi"]
```

with `packages/api/nros-cpp/Cargo.toml:79-81` the twin. RFC-0071's Motivation
called this out by name — *"the asymmetry IS the special case:
`rmw-cyclonedds = ["rmw-cffi"]` carries no `dep:`, because Cyclone arrives
through CMake. A user-facing library encodes one backend's build route in its
feature table."* It is still there, and `uorb` appears in neither manifest at all.

The `nros` umbrella itself is clean on this axis:
`packages/api/nros/Cargo.toml:121` is `rmw-cyclonedds =
["nros-node/needs-type-descriptors"]` — a capability forward with no `dep:`.

## Also unfixed: D8's named leak, and a capability check that landed at one of two sites

* D8 says the platform descriptor's `[knobs.zenoh.tx]` / `[build.zenoh]`
  backend-named sections should key on the resolved backend. They still do not:
  `packages/cli/nros-cli-core/src/cmd/config.rs:120,148,154,160`
  (`board.knobs.zenoh.tx`, `"zenoh.tx.batch"`).
* The `safety` capability is descriptor-driven at
  `packages/api/nros-cpp/CMakeLists.txt:300`
  (`"safety" IN_LIST NROS_RMW_CAPABILITIES`) and still name-driven at
  `cmake/NanoRosFeatureSet.cmake:239` (`if(_FS_RMW STREQUAL "zenoh")`, warning
  "only the zenoh RMW carries the CRC path"). A third-party backend declaring
  `safety = "..."` in its descriptor is honoured on one path and refused with a
  wrong explanation on the other. Classic issue-0196 shape: the fix landed at one
  site of a two-site class.

## Fix direction (not applied)

Write `check-rmw-agnostic` as RFC-0071 specifies, and put it on the fast line.
It must classify prose vs code (the core hits are 90 % migration commentary, so a
raw grep would be unusable) and must cover **CMake and Python as well as Rust** —
two of the five closed lists are in neither Rust nor a manifest, which is why a
manifest-only guard like `check-decoupling.sh` could never have caught them.
Retire or rewrite `check-decoupling.sh` in the same change rather than leaving a
gate that is documented as testing a goal the project abandoned.

---

## Status after phase-439 W4 (2026-09-08) — three of the five lists are gone; the GATE is not written

**Still open, and deliberately so.** W4 was expected to close this issue. It
removed most of what the issue counted and did not write the gate, and saying
that plainly is worth more than a green check on a rule nobody calibrated.

### What went

| site | state |
| --- | --- |
| `CMakeLists.txt:267-402` (`if/elseif` → `FATAL_ERROR`, `uorb` fatal) | **gone** — dispatches on the DECLARED link strategy (issue 1215) |
| `cmake/NanoRosRmwDispatch.cmake` (generated chain + `NROS_RMW_KNOWN` literal) | **gone** — the file asks the CLI (issue 1214) |
| `cmake/NanoRosFeatureSet.cmake:135-146` (name → cffi feature, per crate) | **gone** — reads `[rmw.link] c_cffi_feature` / the derived `<cargo_feature>-cffi` (issue 1216) |
| `cmake/NanoRosFeatureSet.cmake:239` (`safety` is zenoh-only) | **gone** — `"safety" IN_LIST NROS_RMW_CAPABILITIES`, which is what `nros-cpp` already did |

That last one is the 0196-shaped half this issue named: the fix had landed at
one of two sites, and a third-party backend declaring `safety` was honoured on
one path and refused with a WRONG explanation on the other. Both sites now ask
the descriptor.

`scripts/check-entry-rmw-vocabulary.py` no longer regexes `NROS_RMW_KNOWN` out
of a generated file — it reads the `<nano_ros_provides kind="rmw"/>`
announcements, cross-checked against the CLI's own answer by
`rmw_resolver::tests::every_announced_backend_resolves_through_the_scan`.

### What did not: the gate, and why not — MEASURED

`check-rmw-agnostic` was written and then deleted rather than shipped. The rule
is easy to state ("code that enumerates two or more backend names") and the
measurement is what killed the first version:

* over every tracked build-logic file (cmake / rs / py / sh / just / Kconfig),
  comments stripped, names matched only as quoted literals or regex
  alternations: **93 files**;
* narrowed to the build-DECISION surface (`cmake/`, `zephyr/`, `integrations/`,
  `just/`, `packages/{api,boards,cli,core,rmw,platform,tooling}/`): **39 files**.

Most of the excess is not a defect. `packages/testing/**` enumerates the
backends a TEST MATRIX covers; `examples/bridges/tt-zenoh-to-cyclonedds` names
two backends because bridging them is what it demonstrates;
`scripts/build/fixtures-manifest.py`'s `KNOWN_RMWS` is a test-plan coordinate
list. Even inside the 39, spot-checking found roughly 40 % noise from `#[cfg(test)]`
TOML fixtures inside `cargo_metadata_schema.rs` — a shape the reader would have
to learn to skip.

A baseline of 39 (let alone 93) reasons written without verifying each one is a
gate that reads as coverage, which this repo has paid for before. So the gate
needs a classification pass — dispatch vs test-plan vs demo vs help text — and
that is its own wave, not a side effect of W4.

### What a next attempt should start from

* The narrowing that worked: comments stripped per language; names matched only
  where they open a quoted string or follow an alternation (`"zenoh"`,
  `(zenoh|xrce)`), which drops `nros_rmw_zenoh_register` and `use
  nros_rmw_zenoh::…`; a window (~320 chars) so two names at opposite ends of a
  file are not "one construct"; a threshold of TWO names, because one is usually
  a legitimate backend-specific site.
* The name set from `check-entry-rmw-vocabulary.cmake_known()`, imported — a
  gate against second spellings must not carry one.
* What still needs a decision: skipping `#[cfg(test)]` bodies, and whether a
  `Cargo.toml` feature table (`rmw-zenoh = [...]`, this issue's "api leaks")
  belongs to this rule at all or is a separate one.

### Unchanged by W4

The api-manifest leaks (`packages/api/nros-{c,cpp}/Cargo.toml`), the core leaks
(`main_macro.rs`'s `rmw_crate_ident` and its `cyclonedds` filter,
`nros-orchestration-ir/src/cyclonedds_type_sizing.rs`), D8's
`board.knobs.zenoh.tx`, `workspace_scaffold.rs`, `colcon_nano_ros`'s
`RMW_BACKENDS`, and `scripts/check-decoupling.sh` (still documented as testing
a goal the project abandoned).

---

## Resolution — the gate is written, with the RFC's reach (2026-09-11, phase-444 W4.b)

`just check rmw-agnostic` (`scripts/check-rmw-agnostic.py`,
`just/check/rmw.just`) enforces RFC-0071 § Verification as stated:
`packages/core/**`, `packages/api/nros{,-c,-cpp}/**` and
`cmake/NanoRosRmwDispatch.cmake` name no backend outside prose/comments. It is
a fast-line gate — buildless, ~0.4 s, `git ls-files` plus a read — so it runs
on every pull request inside `check-fast`, and on no lane that needs
provisioning.

### The calibration the first attempt lacked

* **Names** are imported from `check-entry-rmw-vocabulary.cmake_known()`
  (`cyclonedds uorb xrce zenoh`) — a gate against second spellings must not
  carry one. They match as token PARTS (`nros_rmw_zenoh`,
  `CONFIG_NROS_ZENOH_LOCATOR`, `rmw-cyclonedds`) but never inside another word,
  so PX4's `uxrce_dds_client` does not match.
* **Not code**: comments per language; PROSE strings — a literal containing
  whitespace is a message to a human (`"E2E message-integrity (CRC) — zenoh
  only"`), while a whitespace-free one (`"zenoh"`, `"nros-rmw-zenoh?/…"`) is a
  value; and TEST code (`tests/` directories, Rust items under `#[cfg(test)]`).
  That is this issue's own measurement applied: a raw census of 421 hits became
  30 files / 151 lines, and then **13 files / 70 lines**, every one of which is
  a real code site. Nothing is baselined that a reader would have to learn to
  skip.
* **Exemptions** are per `(path, name)` with a reason — three, all `uorb` as the
  SERIALIZATION FORMAT (RFC-0088), which this issue counted and explicitly did
  not indict. The same file naming `zenoh` is still caught, and an exemption
  that matches nothing FAILS. That rule earned its place on the first run: a
  fourth entry for `nros-node/src/format_check.rs` was deleted by it, because
  those uses are all inside `#[cfg(test)]`.
* **Debt** is a per-file ratchet: a count may only fall, a fall must lower the
  row in the same change, and every row names an OPEN issue — resolved through
  `scripts/lib/issue_status.py`, so a closed issue cannot hold debt.

### Self-test, on the normal path, every run

14 planted scan cases across Rust (raw strings, char literals, lifetimes,
`#[cfg(test)]` items and a `#[cfg(test)] use …;`), CMake, TOML, C and Python;
the per-name exemption in both directions; the scope predicate; the ratchet
(at baseline / grew / shrank / gone / a new file / a row naming a resolved
issue); and the real tree mutated in memory — `"zenoh"` appended to
`packages/core/nros-rmw/src/lib.rs` must add exactly one line.

Negative controls run against the real worktree, then reverted:

```
$ printf 'pub const BACKEND_NAME: &str = "zenoh";\n' >> packages/core/nros-node/src/lib.rs
packages/core/nros-node/src/lib.rs: 1 code line(s) name a backend, and it is not baselined
    305: [zenoh] pub const BACKEND_NAME: &str = "zenoh";

$ printf 'const EXTRA_RMW: &str = "xrce";\n' >> packages/core/nros-macros/src/main_macro.rs
packages/core/nros-macros/src/main_macro.rs: 6 code line(s) name a backend, baseline 5 (issue 1298) — the debt GREW
```

### Fixed rather than baselined

* `packages/api/nros-c/cmake/NanoRosLink.cmake` — the dead duplicate holding a
  fourth closed list — **deleted** (issue 1218). The gate reported it (4 lines)
  on its first run, so the tree starts one baseline row lighter.
* `scripts/check-decoupling.sh` — this issue asked to "retire or rewrite" it,
  and the MEASUREMENT chose: **it passes today** (`rc=0`, both manifests clean),
  so what shipped was not a gate testing an abandoned goal but an UN-WIRED gate
  nobody could read a verdict from. It is **rewritten and wired**, not deleted:
  - its RMW half is genuinely superseded by RFC-0031 (the `?/` forwarding and
    optional backend deps were deliberately restored) — and is covered from the
    other direction anyway, since `check-rmw-agnostic` reads both manifests it
    read (`packages/api/nros/Cargo.toml`, `packages/core/nros-node/Cargo.toml`)
    and refuses any NEW backend name in either;
  - its PLATFORM half — neither crate may carry a dep / `dep:` / `?/` on a
    concrete `nros-platform-*` — is a live rule with **no other gate**, so
    deleting the file would have dropped it. Narrowed to that half, given a
    self-test on the normal path (one clean manifest, three leak shapes), and
    moved from an advisory `gate.yml` step that only `merge_group`/`schedule`
    ran onto the fast line, where every pull request runs it.

  So nothing it caught goes unchecked: the RMW axis moved to a gate with wider
  reach, the platform axis stayed where it was and now actually runs.

### Baselined, and where each line is tracked

| issue | files | lines |
| --- | --- | ---: |
| 1297 — the API crates select and register backends by name | `nros-c`/`nros-cpp` `Cargo.toml`, `src/lib.rs`, `src/rmw_backend.rs`, `node.hpp`, `nros/Cargo.toml` | 46 |
| 1298 — core names backends | `main_macro.rs`, `nros-orchestration-ir/src/lib.rs` | 6 |
| 1299 — backend-named config in the API crates (D8) | `entry_config.h`, `zephyr/app_config.h`, `nros/src/env.rs` | 18 |

The closed lists OUTSIDE the RFC's reach — `workspace_scaffold.rs`, colcon's
`RMW_BACKENDS`, `board.knobs.zenoh.tx`, `bridge_gen.rs` — are a DIFFERENT rule
("no tool enumerates the backends"), and this issue's own measurement is why
they are not folded in here: 39 files on the build surface, ~40 % of them test
plans, bridge demos and help text, needing the classification pass first. They
are carried by **issue 1300** rather than dropped when this one closed.
