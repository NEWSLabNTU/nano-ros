---
id: 1556
title: "The twelve standalone NuttX C/C++ leaves declare their entities for a
  reason issue 1265 does not describe — no probe REACHES them, and there is no
  model either"
status: resolved
resolved_in: 2026-10-03
type: tech-debt
area: [tooling, build]
related: [1265, 1555, 1142, 0827, 1061, rfc-0098, phase-412]
---

## What

`system.toml` `[[component]] entities` is the third and last live `ENTITIES`
population (populations 1 and 2 — the cmake `ENTITIES` keyword and the
`nros-metadata.json` key — are retired and issue 1555 respectively). Issue 1265
is the standing item to retire it, and it describes ONE reason a leaf must
declare: the host metadata probe exists for this leaf and **cannot run**
(foreign `[build] target` + `build-std`, or a board crate with no host build).

**Measured 2026-09-29: 14 leaves declare, and they split 2 / 12 by reason.**

```
$ for f in $(find examples -name system.toml | sort); do
      grep -qE '^\s*entities\s*=' "$f" && echo "$f"; done
```

| leaves | who | why they declare | covered by |
| --- | --- | --- | --- |
| 2 | `examples/esp32-c3-baremetal/rust/{talker,listener}` | the Rust probe exists for them and cannot run | **1265** |
| 12 | `examples/qemu-armv7a-nuttx/{c,cpp}/*` (6 + 6) | no probe REACHES them, and no model describes them | **this issue** |

Two corrections to the survey this came from, both worth recording:

* it said 15 leaves and 10 NuttX. It is **14 and 12**.
* `examples/workspaces/sizing/src/demo_bringup/system.toml` does **not** declare
  entities — it is a bringup whose line 4 is prose (*"The launch wiring names
  zero callback entities for it"*). It matched a `git grep -l entities`, which
  is not the same measurement.

## Why 1265 does not cover the twelve

Not, as first hypothesised, "C and C++ have no probe". **A C/C++ metadata probe
does exist** — `orchestration::metadata_probe_cmake::run_probes`, which batches
every C/C++ component of a workspace into ONE cmake project (phase-313). The
`examples/workspaces/cpp/src/*_pkg/metadata/*.json.unprobeable` markers are that
probe having been attempted.

The twelve are blocked by something else, and by two things independently:

1. **The probe is WORKSPACE-scoped and these are standalone leaves.**
   `metadata_refresh::refresh_stale_sidecars` enumerates
   `Workspace::discover(ws_root).component_declarations()`. A leaf like
   `examples/qemu-armv7a-nuttx/c/talker/` is a `package.xml` + `CMakeLists.txt`
   + `src/` + `system.toml` with **no `Cargo.toml`**, so nothing enumerates it.
   The evidence is the absence: across all twelve there is no `metadata/` dir
   and no `.unprobeable` marker — they never entered the pipeline, as opposed to
   entering it and degrading.
2. **There is no SystemModel either.** A standalone leaf has no bringup, so the
   contract-sidecar road that replaced the cmake `ENTITIES` keyword in phase-412
   has nothing to resolve. `nros ws entity-facts --leaf <dir>` reading
   `system.toml` is the entire channel, which is exactly what issue 1142 built.

So 1265's fix direction — read the entities from the cross-compiled artifact
instead of from a host rebuild — would not help these leaves, because the
problem is not that the host build fails. Nothing asks.

## What it is worth

Do not retire this declaration casually. Issue 1142 measured, on
`examples/qemu-armv7a-nuttx/cpp/action-client` (arm-none-eabi 13.2.1,
`nros-minsizerel`):

| | `SERVICE_BUFFERS` | image `.bss` |
| --- | --- | --- |
| leaf declares nothing | 35,584 B | 508,208 B |
| leaf declares its one action client | **4,448 B** | 467,248 B |
| delta | **−31,136 B (−87.5 %)** | **−40,960 B** |

A retirement that silently restores the 8-slot fallback guess is a 31 KB
regression on a board with ~512 KB, and the fallback direction is the *safe*
one — the dangerous direction is a declaration that goes stale under-size, which
is what 1265 is about and is equally true here: nothing cross-checks these
twelve, because there is no probe output to check them against.

## A gap this measurement turned up

`find examples -name '*.json.unprobeable'` reports **65** unprobeable components
against **14** declarations. Most of the difference is legitimate — the
`workspaces/cpp` and `workspaces/features` packages have a bringup, so the
contract/model road covers them. But the six `examples/zephyr/rust/*` leaves are
unprobeable cross-only cargo leaves, 1265's exact shape, and declare nothing.

Whether that is a real under-size is NOT established here: CLAUDE.md records
that a standalone Zephyr leaf reaches no descriptor producer and is served by
the four kept `NROS_DECLARED_*` carriers instead. Someone should measure which
of the two it is before treating it as either a bug or a non-issue.

## Fix direction (not decided)

Give a standalone C/C++ leaf the same thing a workspace package has, so the
declaration has something to be checked against or derived from. Candidates,
none measured:

* extend the cmake probe project to a standalone leaf — it already knows how to
  configure a C/C++ component for the host; what is missing is enumeration of a
  non-cargo leaf, not the probe;
* let a standalone leaf resolve a degenerate one-node model from its own
  `system.toml` + a contract sidecar, so the phase-412 road applies and the
  declaration becomes derived rather than authored.

## Acceptance

A NuttX C or C++ standalone leaf with NO `entities` in its `system.toml` gets
the same `SERVICE_BUFFERS` and `.bss` as today's declaring leaf (4,448 B /
467,248 B on `cpp/action-client`, not the 35,584 B fallback), and adding a
service client to its `src/` changes them without touching a declaration.

## Status after issues 1555 and 1265 (2026-10-01) -- still OPEN

Nothing in this issue's own fix direction landed. What moved is around it:

* **Population 2 is gone** (issue 1555, resolved): the `nros-metadata.json`
  `entities` reader is retired and refuses the key. So `system.toml`
  `[[component]] entities` is now the ONLY surface that states what a component
  creates by hand, and nothing about these twelve leaves changed with it.
* **The two esp32-c3 leaves no longer declare** (issue 1265): their node libs
  now build for the host and the Rust probe answers for them, with pool knobs
  byte-identical to what the declaration produced. **The declaring set is now
  exactly these twelve NuttX C/C++ leaves** (`git ls-files '*system.toml' |
  xargs grep -lE '^\s*entities\s*='` -> 12, all under
  `examples/qemu-armv7a-nuttx/{c,cpp}/`).

Why 1265's fix does not carry over, re-checked against the code rather than the
earlier survey: the twelve are `nros_app_main` applications (`NROS_APP_MAIN_REGISTER()`,
the rclc-style C API), not `nano_ros_node_register` components with a class and a
header, so the C/C++ probe's synthesised one-node TU
(`codegen::entry::emit_cpp::emit_typed_probe`) has nothing to instantiate even if
enumeration reached them. The only observer that runs an APPLICATION is the
phase-463 census (`NROS_CENSUS_OUT`), and that switch exists only in the hosted
`nros-cpp` boot funnel of a NATIVE entry (`cmake/NanoRosFeatureSet.cmake` gates
`metadata-mode` on `cpp` + `posix` + not cross) -- a C application and a NuttX
image reach neither. So the remaining work is two pieces, both still open:

1. a census switch in the `nros-c` hosted boot funnel (the C half of
   `NROS_CENSUS_OUT`), so a C application's own `main` can be run as the
   observer;
2. a host configure of a standalone NuttX leaf -- the same `src/` against the
   native board -- that runs it with the switch set, plus the enumeration that
   reaches a non-cargo standalone leaf.

Acceptance is unchanged.

## Status 2026-10-03 -- still OPEN, and the chain is longer than two pieces

Re-read against the code after issue 1419's census became the default check
for workspace images (PR *the census default is refuse*). Reusing the C
workspace's census path -- the generated C entry creates each node with
`nros_cpp_node_create` and its components create entities through the hooked
`nros_cpp_*` ABI -- does NOT carry over to these twelve, because they reach
neither half of it:

1. **Their entities never cross a hook.** A `nros_app_main` application calls
   the rclc-style `nros-c` API (`rclc_node_init_default`,
   `rclc_publisher_init_default`, `nros_timer_init`, ...). The recording
   backend sees its publishers and subscriptions with NO node attribution,
   and its timers not at all -- the census hooks (`on_node_create`,
   `on_timer_create`, ...) live in `nros-cpp`'s `metadata_hooks.rs`, which
   `nros-c` cannot call. That is issue 1419's item "a C node that opens its
   own node through `nros-c`". The shape of the fix: hook bodies move to
   `nros` (both API crates depend on it; bodies `#[cfg(feature =
   "metadata-mode")]`, calls unconditional), and `nros-c`'s node, timer,
   guard-condition and parameter entry points call them --
   `check-census-hooks-complete` then has to hold the `nros-c` entry points
   too.
2. **Nothing stops them before the spin.** The hosted census switch
   (`nros_cpp_census_begin` / `_finish`) sits in the C++ board runners; a C
   application owns its own loop (`rclc_executor_spin_period`). The switch
   needs a C half: `NROS_APP_MAIN_REGISTER()`'s hosted `main`, or the first
   spin call, writes the census and exits when `$NROS_CENSUS_OUT` is set.
3. **A standalone C leaf links no recorder.** `metadata-mode` is on the
   NATIVE C++ umbrella only (`cmake/NanoRosFeatureSet.cmake`); a C leaf's
   native configure would have to link that umbrella.
4. **No host configure of the leaf exists, and nothing enumerates it** -- the
   two pieces this issue already named.
5. **Nothing turns a census into a leaf's pools.** The workspace road compares
   a census with a contract; this road would have to DERIVE from it, where
   `system.toml` `entities` is read today (`--from-leaf`, `leaf_entity_env`).

Acceptance is unchanged. None of 1-5 landed in this session.

## Revised direction (2026-10-03, RFC-0100 Amendment 1)

**The twelve still exist, and still take their own road.** phase-470 unified the
WORKSPACE entries (generated, 15 hand-written Zephyr entries down to 3) and kept
standalone leaves as a deliberate shape (RFC-0026); it did not touch
`examples/qemu-armv7a-nuttx/{c,cpp}/*`. So these remain the only users of RFC-0100
D4's third producer (`--from-leaf`, over `[[component]] entities`), and that
producer retires with this issue, not before.

**Item 1 is the same move as issue 1419's Rust census producer.** The hooks sit
in `nros-cpp`; `nros-c` and `nros-cpp` both depend on `nros`, and a Rust node's
API is `nros`. Hook bodies in `nros` (behind `metadata-mode`, calls
unconditional) reach C, C++ and Rust from one place, so 1-5's first step should
be done once for both issues rather than as an `nros-c` copy of
`metadata_hooks.rs` — a second hook layer is the second spelling this repository
keeps paying for.

**Order.** Item 1 (shared with 1419) → item 2 (the C census switch in the hosted
`NROS_APP_MAIN_REGISTER()` main) → items 3-4 (link the recorder; a host configure
of the leaf and its enumeration) → item 5 (derive the leaf's pools from the
census where `system.toml` `entities` is read today). Until item 5, the
declaration stays authored and `--from-leaf` stays its producer.

Files for item 1: `packages/api/nros/` (new hook module),
`packages/api/nros-cpp/src/metadata_hooks.rs` (becomes calls),
`packages/api/nros-c/src/` (the node / timer / guard-condition / parameter entry
points), `check-census-hooks-complete`. No overlap with issues 1608 / 1647.

## Status 2026-10-03 -- item 1 done (the hooks live in `nros`)

Fixed in the PR that carries this section (*the census hooks move into `nros`*),
shared with issue 1419's Rust item. The four hook bodies moved from `nros-cpp`'s
`metadata_hooks.rs` to `nros::census_hooks` (bodies behind `metadata-mode`,
calls unconditional), and `nros-cpp`'s entry points now call them there; no
`nros-c` copy exists. `nros-c`'s node / timer / guard-condition / parameter
entry points call the same four:

| `nros-c` entry point | hook |
| --- | --- |
| `nros_executor_node_init`, `nros_node_init_ex` (and `rclc_node_init_default`, which forwards) | `on_node_create` (the namespace and domain the node LANDED in) |
| `rclc_executor_add_timer`, `nros_executor_add_timer_in_group` | `on_timer_create` (`Wall` / `Clock` / `InGroup`) |
| `nros_node_create_guard_condition` | `on_guard_condition_create` |
| `nros_executor_declare_param_*` and `*_on` (scalar pairs are `paste!`-generated) | `on_param_declare` |

`nros_parameter_declare_*` (the node-LESS legacy parameter server) is not a
census entry point: it sizes no node's store.

Two behaviour changes the move needed, each tested:

* **A re-opened node is made current again** instead of refused. A tiered Rust
  entry runs `register()` once per tier executor, so the same node is opened
  once per tier; the old refusal is a panic on tier 2.
  (`metadata_mode::tests::reopening_a_node_makes_it_current_again`.)
* **An executor-side entity created before ANY node is counted under an
  executor scope (`__executor__`)** instead of panicking. That is legal C
  (`nros_timer_init` needs only a support context; `nros-c`'s own
  `tests/run/timer_clock_source.c` does it), and with the hooks live in `nros-c`
  it would otherwise panic every native image that links the recorder.

Tests (`just check census-hooks-complete`, the lane that builds `metadata-mode`
with the recording backend): `census_hooks_reach_every_api::{a_c_node_opened_through_nros_c_is_attributed,
an_rclc_node_opens_the_census_cursor, a_node_less_c_timer_is_counted_under_the_executor_scope,
a_rust_component_registered_through_the_runtime_is_attributed}`. Measured red
before: with `nros-c/src` and `nros/src/node_runtime.rs` reverted to `main`, all
three attribution tests fail (the C node is absent from `nodes[]`; the Rust
`register_node` fails `Runtime`, because the recording backend refuses an
entity with no current node). `check-census-hooks-complete` now holds the C and
Rust entry points too (26 hooked, up from 15) and reads `paste!`-generated
names; three new mutations (a C group timer, a generated `declare_param_*_on`,
the Rust `create_node`) each go red.

Items 2-5 are still open.

## Status 2026-10-03 -- items 2 and 3 done (a C application is a census producer)

Fixed in the PR that carries this section (*a C application that owns its own
`main` is a census producer*), on top of item 1.

* **Item 2 -- the C switch** (`nros-c/src/census.rs`). An rclc-style C
  application has no board funnel: it opens its own support context and runs
  its own spin. So the switch rides the two calls every such program makes, in
  order. `nros_support_init*` (all three funnel through `_rmw`) ARMS it before
  the session resolves its backend -- registers the recorder and selects it
  through `$NROS_RMW`, the two halves of `nros-cpp`'s `census_select_backend`
  -- and the first `rclc_executor_spin{,_some,_period,_one_period}` writes the
  census through `nros::metadata_mode::to_json` and exits instead of spinning
  (0 when written). An image without the recorder REFUSES at support init, so
  the run exits non-zero with no file rather than dialling a router.
* **Item 3 -- the recorder is linked.** `nros-c` gains `metadata-mode` (`std`
  + `nros/metadata-mode` + the recording backend), `nros-cpp`'s `metadata-mode`
  forwards to it (that umbrella bundles `nros-c`, and it is the one a native C
  binary links today), and `nros_feature_set` turns it on for the native C
  umbrella too. `check-rtos-feature-set-excludes-analysis`'s rule moved with
  it: `metadata-mode` iff (c OR cpp) + posix + native, with a second negative
  control for the native C set.

Measured on `examples/native/c/talker` (the rclc API: `nros_support_init`,
`rclc_node_init_default`, `rclc_publisher_init_default`, `nros_timer_init`,
`rclc_executor_add_timer`, `rclc_executor_spin_period`), built by the native
fixture lane, run with `$NROS_CENSUS_OUT` and no router: exit 0, census node
`talker` with publisher `/chatter` and one `wall` timer at 1000 ms. Test:
`workspace_metadata::an_rclc_c_application_writes_a_census_without_a_router`
-- measured red with `nros-c/src` at its previous state (exit 1 after the
connect attempt, no file) and green after.

**Still open: items 4 and 5** -- a host configure of a standalone NuttX leaf
(the same `src/` against the native board) with the enumeration that reaches
it, and deriving the leaf's pools from the census where `system.toml`
`entities` is read today. Until then the twelve leaves keep their authored
declarations and `--from-leaf` stays their producer.

## Status 2026-10-03 -- items 4 and 5 for the six C leaves; the declarations stay

Done in the PR that carries this section (*a standalone leaf's census, and
the leaf reader that uses it*).

* **Item 4 -- host configure + enumeration.** `nros ws entity-census take
  --leaf <dir>` configures the leaf's OWN `src/` for the host
  (`-DNANO_ROS_LEAF_BOARD=native`, which `nano_ros_read_leaf_system` passes to
  `nros ws leaf-system --board`; every other row stays the leaf's), builds it
  to its fixed point, runs the program in census mode (item 2's switch) and
  writes `<leaf>/build/nros/census/leaf.census.json`. The census build is not
  sized by what it checks: `NROS_LEAF_CENSUS_HOST` makes the leaf reader
  ignore both the declaration and any census. Provenance is the program, not
  the leaf: the binary, `src/` and `CMakeLists.txt` -- so deleting a
  declaration from `system.toml` does not stale the census, and a source edit
  does.
* **Item 5 -- the reader.** `leaf_entity_env::declared_entities` -- the one
  reader `entity-facts --leaf` (the `NROS_DECLARED_*` carriers of
  `nros_record_leaf_entity_facts`) goes through -- now answers from a CURRENT,
  complete census when the leaf declares no `entities`, and CROSS-CHECKS a
  declaration against a current census when it does (`reconcile`, per-kind
  counts: a stale declaration is refused, naming both). The recorder sees an
  action as its RMW constituents (three services, two topics), so the census
  rows are folded back by the ROS action naming convention before either use.

Measured on all six `examples/qemu-armv7a-nuttx/c/*` leaves (NuttX sources
provisioned; no router): every `take --leaf` writes a complete census, and the
entity facts computed from the census alone (declaration removed from
`system.toml`) are IDENTICAL to the facts from the authored declaration --

| leaf | census | `NROS_DECLARED_*` from census == from declaration |
| --- | --- | --- |
| talker | 1 pub, 1 timer | yes (`SERVICE_SERVERS=0`, `TL_PUBLISHERS=refused:1`) |
| listener | 1 sub | yes |
| service-server | 1 service server | yes (`SERVICE_SERVERS=1`) |
| service-client | 1 service client | yes |
| action-server | 3 srv + 2 pub = 1 action server | yes (`SERVICE_SERVERS=3`, `TL_PUBLISHERS=1`) |
| action-client | 3 cli + 1 sub = 1 action client | yes |

-- so each authored declaration is now a CROSS-CHECKED one wherever a census
is taken. Test:
`entity_census::tests::a_leaf_census_feeds_the_leaf_reader_and_cross_checks_a_declaration`.

**Why no declaration was deleted** ("only where measured equal" -- they are
equal on this road, and deleting is still not safe):

1. The DESCRIPTOR road reads `entities` itself: `sizing-descriptor --from-leaf`
   (`cmd/sizing_descriptor.rs`, the RFC-0100 descriptor work, not this change)
   iterates `[[component]] entities` and writes NO descriptor without them.
   Deleting a declaration today drops the leaf's sizing descriptor and every
   descriptor-first consumer falls back to its default. It has to read the
   leaf census through `declared_entities` (or the same helper) first.
2. No unattended build takes a LEAF census: `census-prepass.sh` takes workspace
   images only. A fixture build of a leaf with no declaration and no census
   would size at the hosted/RTOS defaults -- issue 1142's 31 KB regression.
3. The six C++ leaves cannot produce a census yet: a C++ APPLICATION
   (`nros::init_with_launch_auto` + its own `nros::spin_once` / `wait_for_*` /
   `call`) has no census switch -- only the C++ board funnels do. It needs the
   C switch's two halves on `nros-cpp`'s init and first blocking call.

Until all three hold, `--from-leaf` stays the descriptor's producer for these
leaves and the declarations stay authored -- cross-checked by any census taken.

## Resolution (2026-10-03) -- the twelve size from their census; no list is authored

Fixed in the PR that carries this section (*the twelve NuttX leaves size from
their census*), on top of items 1-5. The three reasons the previous section
gave for keeping the declarations each closed:

1. **The C++ leaves have a census switch.** `nros-cpp` gains the application
   half of `$NROS_CENSUS_OUT`, the sibling of `nros-c`'s `census.rs`:
   `nros_cpp_init_rmw` ARMS it (the same `census_select_backend` the board
   runners call, so the recorder is selected by name and the executor opens at
   the census sizing) unless a runner already selected it, and the program's
   FIRST BLOCKING CALL writes the census and exits -- `nros_cpp_spin_once`
   (which `spin_for`, `spin` and every `Future::wait` loop on), the service
   client's `wait_for_service` and blocking `call`, and the action client's
   `wait_for_action_server`, `send_goal` and `get_result`. An image without
   `metadata-mode` refuses at `nros::init` (non-zero, no file). Test:
   `workspace_metadata::a_cpp_application_writes_a_census_at_its_first_blocking_call`
   (native `cpp/talker` blocks first in `spin_once`, `cpp/service-client` in
   `wait_for_service`) -- measured red with `nros-cpp/src` reverted (`talker:
   exited exit status: 1 and wrote no census`), green after.
2. **The descriptor road reads the census.** A component opts in with
   `entities = "census"` -- one key, so a component cannot state both a list
   and the census. `leaf_entity_env::declared_entities` answers it from a
   CURRENT, complete census and REFUSES otherwise (missing, stale or
   incomplete), naming `nros ws entity-census take --leaf <dir>`;
   `sizing-descriptor --from-leaf` goes through that same reader for an
   opted-in component, so the descriptor and the `NROS_DECLARED_*` carriers
   cannot read two different things. `nros_record_leaf_entity_facts` turns that
   refusal into a configure FATAL_ERROR for an opted-in leaf (its STATUS-and-
   fallback branch stays for every other leaf), and re-configures when the
   census is re-taken (`nros ws entity-census path --leaf`, asked rather than
   spelled). A leaf that did NOT opt in is not read from a census even when one
   is on disk: a sizing moved by someone running `take --leaf` by hand would be
   a derived size nobody asked for (RFC-0100 Amendment 1, ruling 1's
   corollary). Tests: `entity_census::tests::a_leaf_census_feeds_the_leaf_reader_and_cross_checks_a_declaration`
   (opt-in answers; absent says nothing; stale and missing refuse naming the
   command), `leaf_system::tests::entities_census_is_the_census_opt_in_and_nothing_else_is`,
   `cargo_metadata_schema::tests::component_entities_is_a_list_or_the_census_opt_in`.
3. **Unattended builds take the leaf census first.**
   `fixtures-manifest.py census-leaves` lists every `[[fixture]]` leaf whose
   `system.toml` says `entities = "census"`, and `census-prepass.sh` runs
   `take --leaf` for each (once, before the stages, under
   `build-test-fixtures`; and from `fixtures-build.sh` itself, `--only leaves`,
   when that is invoked directly). The leaf's host build lands in
   `<leaf>/build/census-host`, a tree no fixture row builds into.

**The twelve declarations are deleted** -- each `entities = [...]` became
`entities = "census"` -- because the derived facts are MEASURED equal, three
ways, on this host (arm-none-eabi 13.2, NuttX 12.13, Release):

| measured | result |
| --- | --- |
| `entity-facts --leaf`, all 12, list vs census | identical `NROS_DECLARED_*` lines |
| `sizing-descriptor --from-leaf`, all 12 | byte-identical files for 11; `c/listener` differs in one REFUSAL REASON (its census observed `in_place`/`buffered` on the subscription, the leaf road still has no language) -- the `--output-cmake` projection is identical |
| the 12 NuttX ARM images, built by the fixture builder before and after | **every ELF byte-identical** (sha256) |

`cpp/action-client`, issue 1142's subject, on the same tree:

| `system.toml` | `.bss` | `SERVICE_BUFFERS` |
| --- | --- | --- |
| authored list (before) | 241,968 | 300 |
| `entities = "census"` (after) | 241,968 (same ELF) | 300 |
| no `entities` at all (the fallback) | 501,440 | 2,400 |
| `entities = "census"` + an `AddTwoInts` client added to `src/` only | 246,064 | 300 |

(1142's own 4,448 / 467,248 were measured on an older tree; the before/after
pair is what this change answers for.) The last row is the acceptance's second
half: the prepass saw the census stale, re-took it (`4 service client`), and
the image changed with no declaration touched. With the census deleted and the
prepass off (`NROS_CENSUS_PREPASS=done`) the configure refuses:
`` `entities = "census"`, and its census is missing ... Take it first: nros ws
entity-census take --leaf <dir> ``.

Booted under QEMU (`qemu-system-arm` virt, NuttX, `rmw_zenohd`):
`rtos_e2e` `{pubsub,service,action}` x `Nuttx` x `{C,Cpp}` -- 6 passed, on the
census-sized images.

What this does not do: `native/cpp/talker` still carries an authored list
(a native leaf, cross-checked against its probe as before), and no other
standalone leaf (FreeRTOS, ThreadX, Zephyr) opted in -- each would be a sizing
change of its own, with its own runtime evidence.
