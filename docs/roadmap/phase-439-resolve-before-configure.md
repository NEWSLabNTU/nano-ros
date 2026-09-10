# Phase 439 — Resolve before configure

**Status (2026-09-08). Opened from RFC-0094. W0 and W1 are the routing diff and
the digest key — both are PRECONDITIONS and neither changes behaviour. W2–W4 are
the three landings. **W0, W1, W3 and W4 have LANDED; W2 is PARTIALLY landed (the phase, its
artifact, and the entity link's byte agreement — not the deletion). Remainder:
issue 1228 (the second composer) and issue 1252 (the message-bound link, the
one arm still standing, and the deletion it gates).**

W0 corrected three of RFC-0094's own numbers — the count of `package.xml`, the
size of the declare-but-no-file class, and how many of the 21 side-changers are
already handled by another mechanism.

W4 landed out of the stated order because it depends on none of the others: it
is the RMW axis alone (descriptors, the cmake seam, the link strategies), while
W0's and W1's preconditions are about routing and cargo-directory keys.

Implements [RFC-0094](../design/0094-resolve-before-configure.md). Read its
"Design" section first; this doc carries work items and acceptance only.

## Why the order is what it is

Two of the five items exist because a simulation of the proposed workflow found
flaws in it before any code was written. They are listed first for that reason,
not by size.

* **W0 before W3** — routing on `<build_type>` alone hard-fails 20 packages that
  declare a type and legitimately have no build file. The routing diff is the
  test that finds them, and it needs no build.
* **W1 before W2** — the resolve phase makes per-image knob divergence normal;
  without the digest in every cargo-dir key, two images differing only in a
  knob share one directory and the second silently gets the first's numbers.
  RFC-0094 D4. **The phase is worse than the status quo without it.**

## W0 — The routing diff (acceptance A1) — **LANDED 2026-09-08**

`scripts/check/check-package-routing.py`, wired as the fast gate
`just check package-routing`. It pushes every tracked `package.xml` through
RFC-0094 D3's rule and diffs the resulting member / subdirectory lists against
what the three file-presence sites produce today.

* `builder/cargo_root.rs` — `[workspace] members`
* `builder/cmake_root.rs` — `add_subdirectory()`
* `cmd/build.rs` — the cargo-vs-cmake driver choice

Buildless (`git ls-files`), 0.05 s, no baseline list. The full table is
`python3 scripts/check/check-package-routing.py --report`.

### What it measured

The four bucket counts held exactly, on 416 tracked `package.xml`:

    both CMakeLists.txt + Cargo.toml :  21
    CMakeLists.txt only              : 173
    Cargo.toml only                  : 158
    neither                          :  64

**21 packages change side, all of them the class D3 named** — a dual-file
package declaring `nros_cmake`, leaving the cargo members list. Nothing else
moves, and no package's declared type contradicts the single build file it
carries: every `cmake-only` package declares a cmake type and every
`cargo-only` package a cargo type. That last is the finding that makes D3 cheap
— the two questions have not actually diverged anywhere yet.

### Three corrections to the RFC's numbers

* **416 tracked `package.xml`, not 411.** The RFC's four bucket counts sum to
  416 and its prose says 411; the buckets are right.
* **The declare-but-no-file class is 64, not 20.** Composition: 23 `nros_cargo`,
  22 `nros_cmake`, 12 `ament_cmake`, 2 `ament_cargo`, 5 declaring nothing. It is
  not an interface-package accident — it is three systematic families: **34
  bringup packages** (launch + `system.toml`, no build file of their own; 32
  carry a `system.toml`), **13 descriptor packages** (`config/*`,
  `packages/boards/*` carrying `nros-platform.toml` / `nros-board.toml`), and
  **17 interface / message packages**. So D3's
  reason for keeping participation on file presence is three times stronger than
  the RFC states, and the `<build_type>` on all 59 of these is decorative today
  — a bringup declares whatever its workspace's language is, by copy-paste.
* **"Mostly Zephyr Rust leaves" understates how already-fixed the 21 are.** 20 of
  the 21 carry an independent reason cargo membership is impossible or already
  waived, and the gate prints which:
  * **13 declare their own `[workspace]`** — listing one as a member of another
    root is a hard cargo error, measured against a synthetic two-manifest tree:
    `error: multiple workspace roots found in the same workspace`.
  * **13 declare `[package.metadata.nros.entry] deploy`**, which
    `cargo_excluded_entry_dirs` already resolves through the board catalog to
    `Driver::West` and excludes. (Six carry both.)

  So for the 7 workspace Zephyr entries, D3 changes nothing observable: it
  reaches today's answer from the declaration instead of a `Cargo.toml`
  metadata round-trip through the board catalog. The genuine repair is
  `examples/workspaces/mixed/src/rust_heartbeat_pkg` — a cmake-driven Rust node
  (`nano_ros_node_register … LANGUAGE RUST SOURCES Cargo.toml`) that declares
  its own `[workspace]`, so the moment the `mixed` workspace routes to the cargo
  driver the generated root is unusable. That one is a **latent fix the RFC did
  not name**.

### The one row that is neither — RESOLVED by W3, and not the way W0 expected

`packages/rmw/cyclonedds/nros-rmw-cyclonedds` — issue 1224. It is the only
dual-file package with no own `[workspace]` and no entry `deploy`, and it is
member 87 of the repo-root `[workspace]`. Its `package.xml` declares
`nros_cmake` while cargo genuinely builds the crate. W0 read the `CMakeLists.txt`
as a separate C/C++ test wrapper and concluded the declaration was what had to
change before W3 could land.

**It was not, and the premise was false.** W3 measured it: that file is the
production `add_library(nros_rmw_cyclonedds STATIC …)` which the root
`add_subdirectory`s through the backend's own `[rmw.provides.cmake] dir = "."`,
and its CTest harness is `OFF` unless `PROJECT_IS_TOP_LEVEL` — so cmake is
genuinely the driver that enters this DIRECTORY as a package, which is the
question `<build_type>` answers. The crate is reached as a path dependency of
the repo root's HAND-WRITTEN `[workspace]`, which `has_tracked_root` makes the
emitter refuse to regenerate, so W3 could not have moved member 87 whatever the
declaration said. No declaration change, no rule widening; the reasoning is in
`docs/issues/archived/1224-*.md`.

**Acceptance: met.** No build, 21 named-and-justified rows, wired as a gate.
Mutation-tested on the real tree — flipping
`examples/workspaces/rust/src/zephyr_entry` to `nros_cargo` reds the gate naming
it (`{cargo-member, cmake-subdir} -> {cargo-member}`), and flipping
`examples/workspaces/c/src/talker_pkg` to `nros_cargo` reds it on the
intersection rule (`declares a cargo build type and carries no Cargo.toml`);
both restore green.

## W1 — The resolve digest in every cargo-directory key

`nros_share_corrosion_cargo_dir(KEY …)` currently keys on
`features, rmw, board, caps, profile, target`. The Zephyr lane already adds
every `NROS_RESOLVED_*`; the `nros-c` and NuttX lanes do not.

**Acceptance:** two images differing ONLY in a derived knob resolve to different
cargo directories. Mutation-tested — revert the key change and the collision
reappears, demonstrated rather than argued.

**LANDED 2026-09-08.** The knob half has ONE spelling, `nros_knob_key_fields()`
in `cmake/NanoRosSharedCargoDir.cmake`, and all three callers append it. It
reads TWO roads: the resolver registry (`NROS_RESOLVED_KNOBS`, Zephyr today and
every lane once W2 writes `resolved.toml`) and, for the names the resolver did
not answer for, the ENVIRONMENT — which is how `examples/fixtures.toml` states a
knob and the only knob road knowable at key time on a lane with no resolver. The
knob NAME inventory is READ from its one authored site rather than copied: the
43 `_nros_resolve_knob(<NAME> …)` call sites `check-kconfig-knob-forwarding`
already treats as authoritative.

Measured, on the real `nros-c` call site (two root configures, `posix`/`zenoh`,
differing only in `ZPICO_MAX_QUERYABLES`):

| | no knob stated | `ZPICO_MAX_QUERYABLES=2` |
| --- | --- | --- |
| with W1 | `…/d20f4167871d` | `…/ba45165655e7` |
| key change reverted | `…/d20f4167871d` | `…/d20f4167871d` — COLLIDES |
| restored | `…/d20f4167871d` | `…/ba45165655e7` |

The left column is identical in all three rows, which is the other half of the
acceptance: an image whose knobs did not move keeps the directory it already
has, so no warm cargo directory in the tree is invalidated.

The bash-owned fixture group key (`nros_fixture_group_slug`) needed no change —
its signature is already (platform, cargo args, **sorted env**), measured:
`qemu-esp32-baremetal` vs `qemu-esp32-baremetal-4118800323` at
`ZPICO_MAX_QUERYABLES=2` and `-4054584529` at 4. Every other cargo target dir in
the tree is per-build-dir (`${CMAKE_BINARY_DIR}/nros-rust`, `nros-rust-ws-<n>`,
`<ffi-crate>/target`), serves one configuration, and has no key to carry.

Gate: `check-cargo-dir-knob-key`, which drives the production cmake through
`cmake -P` rather than re-deriving the key, and whose NEGATIVE CONTROL runs on
the normal path — with the knob fields removed the two images must COLLIDE, so a
green verdict is a demonstration rather than an assertion.

## W2 — Stage 3.5, the resolve phase — **PARTIALLY LANDED 2026-09-08**

The phase itself: read descriptors and declared entities, run
`EntityInventory::derive` once, write `build/<image>/resolved.toml` with
`[provenance]`. Stages 4 and 5 read it and never re-derive.

Then delete what it replaces: the three-pass convergence in
`zephyr/cmake/nros_cargo_build.cmake`, `nros_reconfigure_settle`, and the
future-mtime arm in `cmake/NanoRosReconfigure.cmake`.

**Acceptance (A3):** Zephyr converges in ONE pass; `check-knob-delivery` still
answers for a built dir; a named image's knobs are byte-identical to today's
before the deletion and after it.

**PARTIALLY LANDED 2026-09-08 (PR #759).** The phase and `resolved.toml` are in
and READ; the three-pass fixed point is NOT deleted and **A3 is not met**.
Remainder is issue 1228.

What landed: stage 3.5 in `plan_builds`, between the preflight bail and the
driver match, writing `build/<bringup>__<image>/resolved.toml` with
`schema_version`, `[image]` (incl. a 16-hex digest over every resolved value),
`[executor]`, `[pools]`, `[entities]` and `[provenance]`. A refusal publishes
prose and NO numbers. `[executor]`/`[pools]` carry raw demand INCLUDING ZERO —
the floor stays at the consumer (issues 1015/1033) — and the two hand-set knobs
name themselves in `[provenance]` with their reason.

The read is `cmake/NanoRosResolved.cmake`, wired at the two sites that READ the
entity-inventory fragment and deliberately not at the three inside
`nros_derive_entity_inventory_knobs()`, which is a producer recording its own
refusal. A lane that ran no resolve phase is unchanged.

Evidence, by pass count rather than by assertion — `just check resolved-seed`,
9/9:

    A  no resolve        1 re-configure   real answer (baseline unchanged)
    B  resolve agrees    0 re-configures  real answer
    C  resolve DISAGREES 1 re-configure   the PRODUCER's answer wins
    D  resolve refused   1 re-configure   byte-for-byte case A

Case B corrected the implementation: the seed first carried a "seeded from …"
banner and measured 1 re-configure, because `nros_reconfigure_snapshot` hashes
CONTENT and one comment line arms the very re-configure the seed exists to
remove. C is the control — a wrong seed must lose.

**A3 is explicitly NOT claimed.** This host has no `zephyr-workspace` and no
Zephyr SDK, so neither "one pass" nor "byte-identical knobs" was measured. Given
phase-392 W5's withdrawn causal claim, saying nothing beats saying it from a
synthetic. Issue 1228 records what the real acceptance is: a `west build` with
`check-knob-delivery <build-dir>` green both sides and unrelated values asserted
unchanged as the control. Its headline is the one differing line between stage
3.5 and the mid-configure producer — `NROS_ENTITY_INVENTORY_SOURCE` — which is
why the pass is not yet saved in the real tree.

**Known gap, not a blocker:** `ZPICO_MAX_QUERYABLES` and
`ZPICO_MAX_LARGE_SUBSCRIBERS` have no declarative derivation (issues 0827, 1061,
1125). They stay hand-set and `[provenance]` says so, which is strictly better
than being hand-set and silent.

### What landed

The **phase and its artifact**, plus the configure-side reader. Not the
deletion — see "What did not", and issue **1228**, which carries the remainder
with the measurement that decides it.

* `packages/cli/nros-cli-core/src/resolve.rs` — `Resolved`, `resolved.toml`
  (D2's shape: `[image]` with a digest, `[executor]`, `[pools]`, `[entities]`,
  `[provenance]`), and `write()`, which renders the TOML and the `include()`able
  `resolved.cmake` from ONE composition. `[provenance]` is normative and carries
  a line per value; `HAND_SET_KNOBS` puts the two undeliverable knobs in it
  BY NAME with the reason, which is the stated known-gap deliverable.
* **Stage 3.5** in `cmd/build.rs`'s `plan_builds`, between the preflight bail and
  the driver match: `resolve_image()`. It reads the image block, the board the
  catalog resolved, and the launch tree's wiring through the resolved
  SystemModel — no compile, no configure, no cargo — and runs
  `EntityInventory::derive` once. **It cannot fail a build:** an image it cannot
  answer for gets a `resolved.toml` recording the refusal and NO projection, so
  every downstream lane behaves as it did before the phase existed.
* `cmake/NanoRosResolved.cmake` — the reader. `nros_resolved_seed_entity_inventory()`
  seeds the entity-inventory fragment from the resolve where the build would
  otherwise write a placeholder, at the two sites that READ it
  (`_nros_load_derived_entity_inventory` in the Zephyr lane, and the
  entity→bounds join in `NanoRosCodegenCore.cmake`). NOT at the three sites
  inside `nros_derive_entity_inventory_knobs`, which are a PRODUCER recording its
  own refusal — seeding those with another composer's answer would publish a
  number the producer did not stand behind.
* `-DNROS_RESOLVED_DIR=<dir>` on the cmake and west handoffs. Passed, never
  guessed: a reader that inferred where a resolve might live would silently pick
  up another image's answer, which is issue 0616 one directory over. A lane that
  ran no resolve phase (a bare `west build`, `just zephyr build-fixtures`) sets
  nothing and is unchanged.
* `just check resolved-seed` / `tests/cmake-resolved-seed-tests.sh`.

### Measured

**The pass count, on a five-line cmake project** (`just check resolved-seed`,
9/9). Its subject is the thing `NanoRosReconfigure.cmake` spends:

| case | re-configures | built with |
| --- | --- | --- |
| A no resolve phase | 1 | the real answer — today's baseline, unchanged |
| B resolve AGREES | **0** | the real answer — the pass this phase removes |
| C resolve DISAGREES | 1 | the **producer's** answer — the safety property |
| D resolve REFUSED | 1 | byte-for-byte case A |

C is what makes it a test. The two composers do not read the same inputs, so a
seed that is wrong must lose; a green A and B with a red C would mean stage 3.5
can ship a number nothing stood behind.

Case B also **corrected the implementation**: the seed first copied the fragment
under a "seeded from …" banner, and measured `re-runs=1` — `nros_reconfigure_snapshot`
hashes CONTENT, so a seed differing by one comment line arms exactly the
re-configure it exists to remove. The seed is verbatim now and the provenance
lives where a byte comparison cannot reach it.

**The two composers agree on every NUMBER and differ by one LINE**
(`resolve::tests::stage_3_5_and_the_mid_configure_producer_agree_on_every_number`,
over the production `merged_per_kind_max` / `to_cmake`): identical values,
identical bytes except `NROS_ENTITY_INVENTORY_SOURCE`, which names the metadata
file on the producer's side and the model alone on stage 3.5's. That one line is
why the pass is not yet saved in the real tree, and it is issue 1228's headline.

### W2.a — the entity link closes, and the pass count is MEASURED (2026-09-10)

Issue 1228's item (3), landed with the first real-image measurement this phase
has had. `demo_bringup:zephyr` from `examples/workspaces/cpp`
(`native_sim/native/64`, zenoh, Zephyr 3.7), clean west build dir each run.

**W2's headline was right about the mechanism and short by one rendering.** The
seed and the mid-configure producer differed in `NROS_ENTITY_INVENTORY_SOURCE`
AND in the per-component provenance line's PACKAGE — `/listener::listener` from
stage 3.5, `listener_pkg::listener` from the merge, because
`EntityInventory::from_model` states the node FQN rather than inventing an ament
package. W2's unit test could not see the second half: it built both composers
with the same `pkg`. So the class is **composer-dependent content in a hashed
file**, and `to_cmake` now emits none of it — no source variable, per-component
rows carrying the component only and sorted on the rendered line. The provenance
is in `entity_inventory.json` and `resolved.toml`'s `[provenance]`, where a byte
comparison cannot reach it.
`resolve::tests::stage_3_5_and_the_mid_configure_producer_agree_byte_for_byte`
asserts whole-string equality between the two production composers, with the
model side built the way `from_model` builds it; restoring either rendering reds
it (both mutations run).

| run | resolve seed | `Re-running CMake` | arms in pass 1 |
| --- | --- | --- | --- |
| no resolve phase (control) | — | 2 | bounds, entity |
| before | yes | 1 | bounds, entity |
| after | yes | **1** | **bounds only** |

Two things that table says and prose kept getting wrong. **The seed already
removed a pass on a real image** (2 → 1) before this wave — W2 declined to claim
it and was right to, and it was true. And **this landing removes an ARM, not a
PASS**: both arms fire in pass 1 and one re-configure discharges both, so the
count moves only when the LAST arm goes. The seed and the producer's fragment now
`diff` empty where they had two hunks.

Control: `python3 scripts/check-knob-delivery.py <build-dir>` green on both
sides; the eight knob values the build reports resolving identical across all
three runs; the image's `zephyr/.config` byte-identical before and after.

**A3 is still NOT met**, and what blocks it is now exactly one link: the
message-bound fragment, which has no pre-configure producer and cannot borrow
the entity link's fix (its composer runs over codegen output during the
configure). That, and the deletion it gates, are **issue 1252**. Items (1) and
(2) of issue 1228 — the producer READING the resolve, which needs stage 3.5 to
gain the refusal guard first — stay on 1228.

### What did NOT land, and what is NOT claimed

* **`nros_reconfigure_settle` and the future-mtime arm are still present and
  still authoritative.** A3 is not met. Issues 1228 (the second composer) and
  1252 (the message-bound link and the deletion).
* **The message-bound half of the chain is untouched** — it is derived
  mid-configure from codegen fragments by a pure-CMake composer with no Rust
  twin, so a second pass survives even once the entity link closes.
* ~~**No claim about a real Zephyr image.**~~ MEASURED 2026-09-10 on a host that
  has one — see W2.a above. The original note, which stood while it was true:
  this host has no `zephyr-workspace`
  and no Zephyr SDK, so "converges in one pass" and "a named image's knobs are
  byte-identical" were NOT RUN. phase-392 W5 had to withdraw a causal claim from
  a before/after that accidentally built the same configuration twice; the
  deletion's acceptance must be a real `west build` with `check-knob-delivery
  <build-dir>` green on both sides and the unrelated values asserted UNCHANGED
  as the control.

## W3 — `build_type` selects the driver — **LANDED 2026-09-08**

Add `build_type` to `PackageXml` and have the three sites read it for the
DRIVER, keeping file presence for PARTICIPATION (RFC-0094 D3). Gate the
intersection: a participating package must have the files its declared type
needs.

**Acceptance:** W0's diff is empty; a package declaring `nros_cargo` with no
`Cargo.toml` that IS routed produces a loud error naming the package, where
before it was silently skipped. **Met, both halves measured.**

### What landed

* `PackageXml.build_type` and `WorkspacePackage.build_type` — the raw spelling,
  never canonicalised in the parser or the scan. The `<build_type>` vocabulary
  has three cross-checked readers already (RFC-0087 D2); a fourth that resolved
  the value would be the drift `check-build-type-spelling.py` exists to stop.
* **`nros_cli_core::routing` — ONE home for the rule**, called by all three
  sites. `route()` is total (a misdeclared package routes NOWHERE, which is what
  makes the separate report load-bearing rather than cosmetic);
  `misdeclaration()` is D3's intersection; `check_declarations()` reports every
  offender at once, the same reasoning `check-tier-preconditions` uses.
* The loud error runs in `cmd/build.rs` beside `check_declared_depends`, i.e.
  **before stage 3 preflight** — so a wrong declaration is diagnosed before a
  toolchain is touched. Both emitters check it again on their own inputs, so a
  caller reaching them by another road cannot get the silent version.
* **`exclude` is now derived from the routing.** A cmake-driven package that
  leaves `members` still has a `Cargo.toml` under the root, and cargo walks UP
  from a manifest — so unlisted-and-unexcluded is an error, not an omission.
  RFC-0094 D3 did not state this; it is the same reason the west entries were
  already excluded. In-tree all 21 side-changers happen to be covered another
  way, so the derivation exists precisely so the next one need not be.

### Measured

**The `mixed` repair, with real cargo.** `examples/workspaces/mixed`'s only
package carrying a `Cargo.toml` is `rust_heartbeat_pkg`, and it declares its own
`[workspace]`:

| the generated root | `cargo metadata --no-deps` |
| --- | --- |
| pre-W3 — `members = [entry, "src/rust_heartbeat_pkg"]` | `error: multiple workspace roots found in the same workspace` |
| post-W3 — that package `exclude`d instead | exit 0, one member |

`nros-cli-core/tests/package_routing_reads_the_declaration.rs` then runs the
REAL emitter over the REAL `mixed` and `rust` trees, so the repair is pinned
rather than re-measured by hand. Its own first version was wrong in the silent
direction — `body.find(']')` matched the `]` of `[workspace]`, so the headline
assertion passed against an empty slice; the negative control over `rust` caught
it, which is the whole reason that control is there.

**The loud error, end to end through the built binary.** Flipping
`examples/workspaces/mixed/src/c_talker_pkg` (cmake-only) to `nros_cargo` and
running `nros build native --dry-run`:

```
Error: 1 package(s) declare a build type they cannot be built by:

  - c_talker_pkg: package.xml declares <build_type>nros_cargo</build_type>, so
    cargo builds it — but there is no Cargo.toml in …/src/c_talker_pkg.
```

Restored, and the build proceeds to its normal preflight.

**The gate's own mutation, both kinds.** Flipping
`examples/workspaces/c/src/talker_pkg` to `nros_cargo` reds
`check-package-routing` naming that exact path; restoring is green. And
reverting `cargo_root.rs`'s member test to `pkg.dir.join("Cargo.toml").is_file()`
reds it at the line — **which it did NOT do in the gate's first version.** Three
greps for `routing::route(` stayed satisfied by that file's OTHER call to the
helper (the `exclude` derivation), so the gate was green while
`a_dual_file_package_declaring_cmake_is_not_a_member` failed in 0.14 s: a gate
that could not fail its own mutation, which is issue 1167's shape. The two
emitters now also refuse an unexplained build-file probe, with
`nros-routing-exempt: <reason>` the one escape — and the selftest checks that
the marker exempts, that a marker further than three lines above does NOT, and
that the predicate quoted inside the comment explaining the rule is prose.

## W4 — Descriptors become load-bearing — **LANDED 2026-09-08**

`nros_rmw_dispatch` becomes a query against the provider index rather than a
generated `if/elseif` chain, following `NanoRosProviders.cmake`'s existing
"cmake asks the CLI" pattern. Both closed lists go — the generated one at
`cmake/NanoRosRmwDispatch.cmake:52` and the hand-written one at the root
`CMakeLists.txt:400`.

Every new configure-time query goes through `nros_codegen_tool_reconfigure()`
(RFC-0094 D6, issue 1018).

**Acceptance (A2):** a fifth backend — a `package.xml` provision, an
`nros-rmw.toml`, a `CMakeLists.txt`, and a C `nros_rmw_acme_register` —
configures and links with ZERO edits to `NanoRosRmwDispatch.cmake`, the root
`CMakeLists.txt`, or the `nros` binary. `uorb` becoming selectable is the
in-tree proof.

Closes issues 1214, 1215, 1216. **1219 stays OPEN** — see below.

### What landed

* `rmw_descriptor.rs` — ONE `nros-rmw.toml` parser, `include!`d by `build.rs`
  and compiled into the library. `resolve_rmw_in(&ScanResult, name)` reads a
  descriptor at SELECTION time over the provider scan, so an out-of-tree
  provider dispatches by the same code as an in-tree one (1214).
* `nros ws rmw-dispatch <name> --lines` / `--known` — the cmake seam.
  `NanoRosRmwDispatch.cmake` is hand-written and ASKS; `render_cmake_dispatch()`
  and its two drift tests are deleted.
* The root chain dispatches on the DECLARED link strategy (`umbrella` /
  `cmake`), one helper for both umbrellas, a per-backend
  `nros-rmw-provision.cmake` hook for what a backend needs resolved first, and
  a generic `NROS_RMW_COMPANION_LIBRARIES` where the root used to name
  cyclone's `libddsc` (1215).
* The three unread dispatch outputs: `EXTRA_LINK_LIBS` deleted, `RLIB_DEP` and
  `UMBRELLA_CFFI_FEATURE` wired — the latter deleting a fourth closed list in
  `NanoRosFeatureSet.cmake`. `LINK_DEPENDS` keys on the declared cmake target
  rather than the `nros_rmw_<name>` naming coincidence (1216).
* `check-codegen-tool-reconfigure` counts `rmw-dispatch` as an emitting verb,
  with its own negative control.

### Measured

`NANO_ROS_RMW=uorb` configures and reaches a correct link line — it had been
advertised in the cache drop-down and fatal at the root. A uorb IMAGE still does
not link on a plain host: `orb_*` is PX4's uORB middleware, not ours. All four
backends configure; the cargo feature sets are byte-identical to the ones the
deleted closed chains produced; cyclonedds and zenoh and xrce probes LINK, and
`ninja -t query` shows both the backend archive and `libddsc` under `|`.

### Why 1219 is not closed

`check-rmw-agnostic` was written and deleted rather than shipped. W4 removed
three of the five closed lists it counted and fixed the `safety` capability's
two-site defect, but the GATE needs a classification pass the measurement made
plain: repo-wide over build logic the rule reports 93 files, and 39 even
narrowed to the build-decision surface, most of them test plans, demos and help
text rather than dispatch. A 39-row baseline of unverified reasons reads as
coverage. The survey, the narrowing that worked, and what a next attempt should
decide are recorded in the issue.

## Out of scope

Per-package independent builds (RFC-0094 "Out of scope"), the `std` deletion,
the unsafe census, and workspace membership. Filed as 1208–1221; none blocks
this phase.
