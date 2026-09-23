# phase-463 - the host census: the code's entities, compared with the contract before any image is built

**Status (2026-09-21). PROPOSED - nothing landed. W0-W7 are open. W7 (the
profiling half) is deliberately last and is a separate decision from W1-W6.**
Opened from the safety-island experiments of 2026-09-20 (E3a/E3b/E3c in the
island's experiment brief) and from the layer map they produced. Home phase for
[issue 1419](../issues/1419-no-layer-reconciles-contract-endpoints-with-the-code.md).
Successor to [phase-308](archived/phase-308-cpp-metadata-producer.md) and
[phase-313](archived/phase-313-workspace-scoped-metadata-probe.md) (the C/C++
recorder and its probe, both landed and both, it turns out, not wired to
anything that compares), and the verifying half that
[phase-403](phase-403-type-bound-rx-sizing.md) W9 promised: "the declaration
supplies and the running image verifies". On the island the running image is
the S32K344, and its verification is `ExecutorFull` at boot with no console.
This phase moves the verifying half to the host, where it can refuse a build.

## Parallel plan

Eight units, one claim id each (`just claim phase-463-Wk`; advisory, TTL in
hours, an open PR supersedes it). W0, W1 and the funnel half of W2 start
now on disjoint files; W3 through W6 form a chain because each compares or
gates what the one before produces; W7 is behind its own go/no-go decision
as the status line says. W1 and W5 both touch `metadata_hooks.rs`, W2 and W7
both touch the hosted funnel in `nros-cpp/src/lib.rs`, and W3 and W4 both
extend the `system.toml` schema: each pair is serialised below, not merged,
because the later wave has a gate of its own. Every path below exists in the
tree today except the ones marked new.

| claim id | depends on | owns | gate | starts now? |
| --- | --- | --- | --- | --- |
| `phase-463-W0` | none | this document (the measurement table); it reads the island and edits no code | the table, one measured row per island component; `python3 scripts/check-roadmap-claims.py` stays green | yes (one day) |
| `phase-463-W1` | none | `packages/rmw/metadata/src/lib.rs`, `packages/api/nros-cpp/src/metadata_hooks.rs`, `packages/api/nros/src/node_metadata.rs` (schema v2), the hook call sites in `packages/api/nros-cpp/src/params_shim.rs` (the `nros_cpp_node_declare_param_*` family), `packages/api/nros-cpp/src/timer.rs` and `packages/api/nros-cpp/src/guard_condition.rs`, a new recipe `census-hooks-complete` in `just/check/codegen.just` | the new `census-hooks-complete` recipe under the check module (new; the fixture component, and the remove-one-hook negative control) plus the phase-308 layer grep | yes |
| `phase-463-W2` | `phase-463-W1` for its acceptance (21 parameters need the param hook); on `emit_cpp.rs` after `phase-459-W2` and `phase-462-W1` | `packages/api/nros-cpp/src/lib.rs` (`nros_board_native_run_components_named`, :1381), `packages/boards/nros-board-linux/src/lib.rs` (`boot_hosted`, :273), `cmake/NanoRosFeatureSet.cmake` (the native umbrella adds `metadata-mode`), `packages/cli/nros-cli-core/src/cmd/entity_census.rs` (new) plus its two dispatch lines in `packages/cli/nros-cli-core/src/cmd/ws.rs` (:158, :499), `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs` (at most the native funnel call) | `nros ws entity-census run --entry <name>` on the island: 11/14/2/2/4/21 across four nodes in under a second, no router; `nm` equal to the boot binary | yes on the funnel; acceptance after W1 |
| `phase-463-W3` | `phase-463-W2` (the census to compare), `phase-460-W1` (the check opens a model, so it calls the `model_gate` 460 W1 introduces) | `packages/cli/nros-cli-core/src/entity_census.rs` (new: the join, the verdict table, the waivers), the `check` subcommand in `cmd/entity_census.rs`, `[census.waive]` on `SystemToml` in `packages/cli/nros-cli-core/src/orchestration/cargo_metadata_schema.rs`; it reads `build/nros/entity_inventory.json` and owns no line of `entity_inventory.rs` | the island experiment table re-run as unit tests in `packages/cli/nros-cli-core`: E3a/E3b/E3c/E4/E2a each refuse with the named verdict, the pristine contract passes with 33 + 21 confirmed rows | no |
| `phase-463-W4` | `phase-463-W3`; on `ws.rs` after `phase-460-W1` | a new recipe `entity-census` in `just/check/codegen.just`, `packages/cli/nros-cli-core/src/cmd/ws.rs` (the `sync: source metadata` block, :3245-3253), `packages/cli/nros-cli-core/src/orchestration/metadata_refresh.rs` (the digest walk, reused), `cmake/NanoRosEntry.cmake` (the `--require-fresh` call after `nros_record_entity_facts`, :381), `[census] on_missing` / `on_stale` in `cargo_metadata_schema.rs` | the new `entity-census` recipe under the check module (new); the stale-source, re-run, add-row, touch-only sequence | no |
| `phase-463-W5` | `phase-463-W2` for I1 and I3(b, c); I2 and I3(a) need nothing landed | new recipes `census-no-conditional-api` in `just/check/abi.just` and `rtos-feature-set-excludes-analysis` in `just/check/platform.just`, `packages/api/nros-cpp/src/metadata_hooks.rs` (the `#[inline]` empty bodies, after W1), the two I3(c) numbers recorded in this document | the two new recipes, `just check api-parity`, the image-facts lane unchanged to the byte | yes for I2 and I3(a) |
| `phase-463-W6` | `phase-463-W3`, `phase-463-W4` | `packages/core/nros-orchestration-ir/src/executor_sizing.rs` (`count_callbacks_with_recorded`) and its caller `packages/cli/nros-cli-core/src/orchestration/model_ingest.rs` (:406); `packages/core/nros-macros/src/main_macro.rs` (:988) keeps the sidecar reader and is not edited; the island half is `island-W2` in the island's own phase doc | `cargo test -p nros-orchestration-ir`; the new `entity-census` recipe under the check module green on the island after the flip | no |
| `phase-463-W7` | `phase-463-W2`, `phase-463-W5`, and the separate decision the status line names | `packages/core/nros-node/src/executor/spin.rs` (the dispatch hook behind `profile-mode`), `profile-mode` in `packages/core/nros-node/Cargo.toml`, `packages/api/nros/Cargo.toml` and `packages/api/nros-cpp/Cargo.toml`, the `NROS_PROFILE_OUT` switch in `packages/api/nros-cpp/src/lib.rs` (after W2), `docs/design/0078-wcet-is-declared-per-profile.md` (the host-profile amendment) | the 60 s replay on the island's native image: rows match the census one-to-one, the observed `paths` outputs equal the contract's, the Zephyr bake refuses the host profile | no |

Files two waves touch, and the order they serialise in. Within this phase:
`metadata_hooks.rs` is W1 then W5; `packages/api/nros-cpp/src/lib.rs` is W2
then W7; `cargo_metadata_schema.rs` is W3 then W4; this document's status
lines are one-line edits by every wave, land order, no dependency. Across
phases: `packages/cli/nros-cli-core/src/cmd/ws.rs` is phase-460 W1 first
(it moves `model_provenance_stale` out of the file into `model_gate.rs`),
then W2 here (two dispatch lines), then W4 here (the freshness line, which
belongs beside the gate module 460 W1 creates); `cmd/model_path.rs` has one
writer, 460 W1, and the census staleness refusal here lives in
`cmake/NanoRosEntry.cmake`, which 460 W1 does not edit. `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs`
is phase-459 W2 (the `run_tiers` tail), then phase-462 W1 (the monitor table
installed before entity creation), then W2 here, then phase-461 W6 (the
registration call at :1086). W2 here goes after 459 W2 and 462 W1 because
both change what the generated entry does before spin and W5's I1 gate says
the census binary IS the boot binary, so the census must be written against
the entry's final pre-spin sequence rather than re-proved after each; and
because W2's own edit to the emitter is at most one line at the native
funnel call, the cheapest of the three to rebase. `packages/api/nros-cpp/src/params_shim.rs`
is W1 here (the `on_param_declare` call) then phase-461 W6 (the feature
split). `packages/cli/nros-cli-core/src/entity_inventory.rs` is owned by
phase-460 W3, phase-461 W3 and phase-461 W6 in that order; W3 and W6 here
read the inventory JSON and own no line of it, and if W3 needs a field added
to the JSON it goes after 461 W3. Phase 457 shares no file with this phase.

Relates to [RFC-0100](../design/0100-rmw-agnostic-sizing-model.md) (the
contract states the facts), [RFC-0078](../design/0078-wcet-is-declared-per-profile.md)
(a WCET belongs to a profile, not to code), [RFC-0063](../design/0063-system-model-is-a-build-artifact.md)
(a derived artifact carries its inputs' digests) and
[RFC-0046](../design/0046-launch-authoritative-node-identity.md) (node identity
is resolved once, in `Executor::node_builder`).

## Why this phase exists

The contract sidecar (`<bringup>/launch/<stem>.contract.yaml`) is the sole
source for every pool size an image compiles with: `nros ws entity-inventory`
counts subscriptions, publishers, queryables, callbacks and liveliness tokens
from it, `nros-node/build.rs` sizes the arena from it, and the inventory header
says on purpose that it carries NO headroom, so that a stale declaration fails
entity creation at boot rather than being absorbed. That design assumes the
declaration is TRUE. Nothing on the host checks that it is.

Measured on the safety island (four C++ nodes, one contract, native and
Zephyr images), three edits to the contract, none touching the code:

| edit | what every host layer said | what the S32K344 would say |
| --- | --- | --- |
| E3a: delete one `sub:` row and its topic (the 2026-09-04 bug) | resolver 0 errors; inventory one short in every pool (10 subscriber slots, 18 callback slots, 57 liveliness tokens, a 46,976 B arena); entry codegen rc=0 | `ExecutorFull` at boot, on a board with no wired console |
| E3b: declare a `phantom` sub the code never creates, wired to a topic | 0 errors; every pool one LONGER; 3,664 B more arena | boots; the over-provision is never found |
| E3c: declare the same sub under `sub:` but wire it to no topic | 0 errors; the model gains an endpoint; the inventory does not change at all | nothing, ever: the endpoint is dropped with no diagnostic |

The one host-time cross-check that exists, the declared-QoS `static_assert` in
`NROS_SUBSCRIBE`, is keyed on the CODE's call sites, so it cannot see an
endpoint the code never mentions (E3b, E3c) and it cannot see that an endpoint
the code does mention is absent from the contract (E3a): the table's own rule
is "an ABSENT row is nobody declared this endpoint, and nothing asserts against
it". The declared-params check (phase-446 W6) has the same shape for
parameters. Both are the right checks for what they check. Neither is a
reconciliation.

### What already exists, and why it does not close the gap

This is not a green field, and the reason it looks like one is the defect.
phase-308 built a recorder for exactly this fact - "which entities does this
component create" - and phase-313 built the probe that runs it for C/C++
components inside `nros sync`. Read against the island today:

1. **The recorder is the right seam.** Every entity a C++ node creates crosses
   the C++ -> Rust ABI in one of nine functions
   (`nros_cpp_publisher_create`, `nros_cpp_subscription_create`,
   `nros_cpp_service_server_create`, `nros_cpp_service_client_create`,
   `nros_cpp_action_{server,client}_create`, `nros_cpp_timer_create*`,
   `nros_cpp_guard_condition_create`, `nros_cpp_node_create_ex`), and the
   RMW-bound ones reach a backend selected BY NAME (`NROS_RMW=metadata`).
   That is nano-ros's analogue of play_launch's `LD_PRELOAD` on
   `rcl_publisher_init` / `rcl_subscription_init` (play_launch phase 77), one
   layer lower and with no preload: the hook is inside the runtime the C++
   already links. User code is untouched by construction.

2. **The recorder drops the QoS.** `nros-rmw-metadata`'s `create_subscriber`
   takes `_qos` and never reads it (`packages/rmw/metadata/src/lib.rs:131`).
   The island's two surviving sidecars (dated 2026-09-11) say
   `/control/command/control_cmd` has `depth: 10`; the code says
   `::nros::QoS(1)` and the contract says 1. The number in the sidecar is a
   default, not an observation.

3. **Parameters are invisible.** The three hooks are node, timer and guard
   condition (`packages/api/nros-cpp/src/metadata_hooks.rs`). Nothing observes
   `Node::declare_parameter`, so the sidecar's `parameters: []` is not "this
   node declares none" - it is "nobody looked". The island's nodes declare 21.

4. **The probe does not build on the island.** `nros sync` reports "no producer
   for autoware_mrm_handler::mrm_handler (metadata probe build failed ...
   exit 2)" for all four components: the phase-313 batch project generates one
   FFI glue crate per component package and they collide on the shared
   `builtin_interfaces_msg_duration_t` (`error[E0428]: defined multiple
   times`). Issue 0641's negative cache then stops retrying, and sync goes on
   without a producer, as designed: "a sidecar-less bake falls back to the
   SystemModel bound". On the island the fallback has run since September.

5. **Even when it runs, nobody compares.** The only consumer of a recorded
   count is `count_callbacks_with_recorded` (`nros-orchestration-ir/src/executor_sizing.rs:154`),
   which takes `max(modelled, recorded)` per node for ONE number, the
   executor's callback total. A max cannot report a disagreement; it hides
   one. `nros ws entity-inventory` - the thing that actually sizes the pools -
   reads `nros-metadata.json` (class, header, shape, callback groups) plus the
   model, and never opens a recorded sidecar at all
   (`packages/cli/nros-cli-core/src/entity_inventory.rs`, no reference).

6. **The probe is not the entry.** The probe constructs one component at a
   time with no launch parameters seeded. The island's `stop_mode_operator`
   computes its timer period from `declare_parameter<double>("rate", 30.0)`,
   which the launch file overrides; the generated native entry seeds that
   value through `nros_cpp_declare_param` before construction, and a bare
   probe cannot. The entity set is the same either way (a node cannot create
   an entity conditionally on a parameter without lying to its own contract),
   but the timer PERIOD the census reports must be the launched one.

So the seam is right, the recorder is incomplete, the producer is broken on
the reference consumer, and the comparison does not exist. This phase fixes
those four things in that order and adds nothing to the RTOS image.

### The direction, in the owner's words

Two intended ways, both to be designed: compile the HOST binary of the same
node code and run it under analysis and profiling instrumentation, so the
entities the code creates (and later, its timing) are observed and compared
with the contract; and insert analysis/profiling hooks into the code path
itself (the C++ API or the node runtime) so the census is produced at build or
first run. Hard constraint: users write their node code for the RTOS only;
the host build must need no source changes and no ifdefs, and nothing added
may change the RTOS image's behaviour or footprint when analysis is off.

Read against the tree, the two ways are the same design seen from two ends:
the hooks (way 2) are what the host binary (way 1) runs. W1 is the hooks, W2
is the host binary, and the invariants in W5 are what make them one thing
rather than two.

## What it does

A **census** is a JSON artifact, `build/nros/census/<entry>.json`, produced
by running the native image of an entry in census mode. It lists, per node
(keyed by the launch-authoritative FQN, RFC-0046): every publisher,
subscription, service server, service client, action server, action client
(kind, name as the code spelled it, resolved name, type, the QoS the code
passed after overrides - all four policies plus depth), every timer (kind:
wall / clock / oneshot, period in ms as launched), every guard condition, and
every parameter the code declared (name, type, code default). It carries
provenance in the RFC-0063 shape: a digest per component source tree, the
generated entry TU's digest, the model's own `meta.inputs` digests, the
recorder's schema version and the CLI that wrote it.

A **census check** compares three documents - the census, the contract as
authored, and `build/nros/entity_inventory.json` as derived - and emits one
verdict per (node, kind, name). It refuses on any verdict of severity `error`.

Neither touches the RTOS road. The census is a run mode of the hosted boot
funnel, gated on a cargo feature that only the native umbrella enables.

## Waves

### W0 - measure the reference consumer before changing anything

Run the existing probe on the island and record, per component, what it
produces today: which of the four fail (all four; W0 measured the cause as the
unbounded-field static assert on VelocityReport/Odometry header.frame_id, since
the batch probe has no system.toml or capacities, not the E0428 collision this
document first assumed), what the two stale
sidecars claim (depth 10 where the code passes 1; `parameters: []` for nodes
that declare 2 to 8), and what `count_callbacks_with_recorded` computed for
the last bake. Also record that the island's own `just build` exports
`NROS_EXECUTOR_MAX_CBS=32` for the native image, so the native image is hand
over-provisioned and would not have shown E3a even at boot.

Acceptance: a table in this document with one row per island component, each
cell measured rather than inferred. This is the baseline W3's acceptance is
measured against.

Claim: phase-463-W0. Depends on: none. Owns: this document (the measurement table). Gate: the table, one measured row per island component. Status: measured 2026-09-21, table in PR #1194; the island probe fails on the unbounded-field static assert, not E0428, at f0d191c98.

### W1 - the recorder tells the whole truth

Hooks are the census. Three changes to the phase-308 adapter, all behind the
existing `metadata-mode` feature, all with unconditional call sites and
`#[cfg]` bodies exactly as `metadata_hooks.rs` does today:

1. `nros-rmw-metadata` records the QoS it receives - reliability, durability,
   history, depth, deadline, lifespan, liveliness - on every endpoint kind. The
   value recorded is the one AFTER `apply_qos_overrides`, because that is the
   one the executor would have sized for; the source spelling is kept beside
   it, because that is the one a contract author reads.
2. A fourth hook, `on_param_declare(name, type, default)`, called from the
   `nros_cpp_node_declare_param_*` family, attributed to the current node via
   the phase-308 node cursor. C++ has no call that declares a parameter
   without crossing this ABI, so the hook is complete by construction.
3. Timer kind (wall / clock / oneshot / in-group) recorded beside the period,
   and guard conditions get their own kind rather than being counted as
   timers. The sizing consumers still read one slot each; the census check
   reads the kind.

Schema `version: 1 -> 2` in `nros::node_metadata`, serialised in exactly one
place as phase-308's layer rule requires: the hooks and the backend contain no
JSON, no schema struct, no slot arithmetic. Gate: the phase-308 grep that
enforces that rule still passes.

Acceptance: a fixture component with one subscription at `QoS(1)`, one
publisher at the default, one wall timer, one guard condition and two
parameters produces a sidecar with exactly those facts, and the negative
control holds: remove any one hook call and the fixture's census fails to
match, so a hook that quietly stops being called is caught.

Claim: phase-463-W1. Depends on: none. Owns: packages/rmw/metadata/src/lib.rs, packages/api/nros-cpp/src/metadata_hooks.rs, packages/api/nros/src/node_metadata.rs, the hook call sites in packages/api/nros-cpp/src/params_shim.rs, packages/api/nros-cpp/src/timer.rs and packages/api/nros-cpp/src/guard_condition.rs, a new census-hooks-complete recipe in just/check/codegen.just. Gate: just check census-hooks-complete (new) plus the phase-308 layer grep. Status: landed in PR #1194 (d9bcddbc6, merged 2026-09-22); check-census-hooks-complete OK (14 entry points, 4 RMW seams, 7 mutations red).

### W2 - the native entry is the census producer

The census is produced by the entry's own native binary, not by a
per-component probe. The generated native entry already does everything a
census needs in the order boot does it: register the linked backend, open the
executor, seed every launch parameter through `nros_cpp_declare_param`,
placement-new every component in launch order, register the parameter
services, then spin. Census mode is that sequence with "dump and exit 0"
where "spin" is.

The switch lives in the hosted boot funnel (`nros_board_native_run_components_named`
in `packages/api/nros-cpp/src/lib.rs`, and `LinuxBoard::boot_hosted` for Rust
entries): when `NROS_CENSUS_OUT=<path>` is set in the environment, the funnel
selects the recording backend by name (the same `NROS_RMW=metadata` the probe
uses), runs setup, calls `nros_cpp_metadata_dump` into `<path>`, and exits
without spinning. The environment is the right switch here and would be the
wrong one anywhere else: the hosted funnel is the ONE place in the tree that
already resolves through the `env` capability (issue 0687), and no RTOS board
has that capability, so the switch does not exist on the RTOS road at all
rather than being compiled out of it.

The native C++ umbrella (`NanoRosCpp` built for `DEPLOY native`) gains
`metadata-mode` in its feature set. The RTOS umbrellas do not, and W5 gates
that they cannot. Issue 0304 is the precedent for why the feature set has to
be checked and not assumed: a `set(NROS_EXTRA_CPP_FEATURES "metadata-mode")`
once reached `nros-c` too, and `metadata-mode` exists only on `nros-cpp`.

`nros ws entity-census run --entry <name>` builds the native entry if needed,
runs it with the two variables set, and writes
`build/nros/census/<entry>.json` with provenance. The phase-313 probe stays
for leaf packages that have no entry and no bringup (a single Node pkg whose
sidecar the `nros::main!` macro reads); for an entry with a bringup, the
census supersedes the probe, and the E0428 collision on the island stops
mattering because the island no longer takes that road.

Acceptance: on the island, `NROS_CENSUS_OUT=/tmp/c.json NROS_RMW=metadata
./build/src/native_entry/native_entry` writes a census listing 11
subscriptions, 14 publishers, 2 service servers, 2 service clients, 4 timers
and 21 parameters across four nodes, in under a second, with no router
running. Same binary as a normal boot runs; `nm` of it is unchanged by the mode.

Claim: phase-463-W2. Depends on: phase-463-W1 (acceptance), phase-459-W2, phase-462-W1. Owns: packages/api/nros-cpp/src/lib.rs (the hosted funnel), packages/boards/nros-board-linux/src/lib.rs (boot_hosted), cmake/NanoRosFeatureSet.cmake, packages/cli/nros-cli-core/src/cmd/entity_census.rs (new) and its dispatch lines in packages/cli/nros-cli-core/src/cmd/ws.rs, packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs (at most the native funnel call). Gate: nros ws entity-census run on the island, 11/14/2/2/4/21 in under a second, nm equal to the boot binary. Status: PR #1221; the hosted funnel is the producer (`$NROS_CENSUS_OUT`, both runners) and `nros ws entity-census run` wraps what it writes with RFC-0063 provenance. Gated in-tree rather than on the island: `census_funnel_tests` drives W1's fixture sequence through the real ABI as the setup function the funnel calls, and the file the RUN writes carries exactly what W1's recorder saw -- schema v2, depth 1 and depth 10, one wall timer at 33 ms, one guard condition, two parameters. `nm` of an RTOS-featured staticlib (thumbv7em, no `env`, no `metadata-mode`) finds 0 census / metadata symbols; the native umbrella carries `nros_cpp_metadata_dump` and 0 symbols the MODE adds, which is I1's nm equality stated as an absence. The island's 11/14/2/2/4/21 was NOT re-run: this worktree builds no island image. A RUST entry's funnel (`boot_hosted`) reads the switch and REFUSES naming the cause, because W1's hooks are on the C++ ABI and a Rust entry links no recorder.

### W3 - the comparison, as verdicts with severities

`nros ws entity-census check --census <json> --contract <yaml> --inventory
<json> --model <yaml>` joins the three views and emits one row per (node FQN,
kind, name). The verdict vocabulary is play_launch phase 77's, extended with
the two directions the island showed matter:

| verdict | meaning | severity | why that severity |
| --- | --- | --- | --- |
| `confirmed` | the code created it, the contract declares it, and every fact agrees | - | - |
| `missing-in-contract` | the code created it; no `sub:`/`pub:`/`srv:`/`cli:`/`paths:` row names it (E3a) | error, not waivable | every pool derives one short; the first catch is `ExecutorFull` on a board with no console |
| `phantom` | the contract declares and wires it; the code never created it (E3b) | error, waivable per row | safe for memory, but a false statement about the code, and it feeds the rate hierarchy and causal checks that then verify a graph that does not exist |
| `unwired` | declared under a node's `sub:`/`pub:` but absent from `topics:` (E3c) | error | the resolver drops it silently; the model and the inventory never see it |
| `depth-mismatch` | the code's history depth differs from the declared one | error | the arena is sized from the declaration (phase-403 step 2); this is the -403 refusal, one build earlier and in both directions |
| `qos-mismatch` | reliability / durability / history differ | error | RFC-0100 W3: a `keep_all` or transient-local endpoint prices differently |
| `type-mismatch` | the code's message type differs from the contract topic's `type:` (E4) | error | the bound inventory priced the wrong type |
| `period-mismatch` | a timer's launched period disagrees with `paths.<p>.trigger.timer.rate_hz` | error | the tier derivation and the rate hierarchy read the declared rate |
| `param-missing-in-contract` | the code declares a parameter the node's `params:` does not | error | the store has no slot counted for it; the -446 refusal, one build earlier |
| `param-phantom` | `params:` names a parameter the code never declares | error, waivable | a launch value that can reach nothing |
| `param-type-mismatch` | name agrees, type differs | error | play_launch's own check rejects the launch value; the store's reader rejects the write |
| `unobserved` | the contract declares it and the census never ran this node | warning | evidence absent is not evidence of absence (RFC-0078's rule, applied to structure) |

Parameter services (six queryables per node) are registered by the entry,
not by node code, and the inventory counts them as `infra_queryables`; the
check excludes them by kind rather than by name.

Severity semantics, stated once: a verdict in the UNDER direction (the code
has more than the contract says) is an error with no waiver, because it is
the class that ships an image that dies at boot. A verdict in the OVER
direction (the contract says more than the code has) is an error by default
because a contract is a statement about the code and a false one is a defect,
but it is waivable per row with a reason, in `system.toml` under
`[census.waive]` - never in the contract, whose parser rejects unknown keys
and whose schema is rlm's. A waiver names the row and the reason; an unnamed
row is not waived. `--strict` turns warnings into errors for the merge queue.

Acceptance is the island's experiment table, re-run: E3a refuses with one
`missing-in-contract` row naming `/mrm_handler/operation_mode_state`; E3b
refuses with one `phantom` row; E3c refuses with one `unwired` row; E4 refuses
with `type-mismatch`; E2a refuses with `depth-mismatch` naming both numbers;
the pristine contract passes with 33 `confirmed` entity rows and 21
`confirmed` parameter rows and zero warnings. Each refusal names the node,
the entity, the file that should change, and the line to add or remove.

Claim: phase-463-W3. Depends on: phase-463-W2, phase-460-W1. Owns: packages/cli/nros-cli-core/src/entity_census.rs (new), the check subcommand in packages/cli/nros-cli-core/src/cmd/entity_census.rs, the [census.waive] table in packages/cli/nros-cli-core/src/orchestration/cargo_metadata_schema.rs. Gate: the island experiment table as unit tests in packages/cli/nros-cli-core (E3a/E3b/E3c/E4/E2a refuse, pristine passes 33 + 21). Status: PR #1226, in the queue; every delta between contract and code is a named verdict with a severity. On the island fixture: 53 confirmed, 1 error, 0 warnings, 0 waived, and the error is the island's own historical defect, a contract declaring six subscriptions where the code creates seven. The verdict names the missing one, writes the `sub:` row that would fix it, and refuses a waiver, because every pool derives one short and the first catch would be `ExecutorFull` on a board with no console. Waivers live in `system.toml [census.waive."<node>:<kind>:<name>"]` and a test holds that rlm's contract parser rejects a `census:` key. Two doc corrections: `unwired` is NOT waivable, there being no true statement a waiver could stand behind, and the row is keyed by resolved topic because the endpoint alias is not derivable.

### W4 - where it runs, and how it goes stale

Three places, one artifact:

1. **An explicit gate**, `check-entity-census`, in the `check` family and in
   the island's justfile: run the census, then the check, `--strict`.
2. **`nros sync` does not run it.** Sync resolves models and refreshes
   sidecars; it does not build binaries, and the census needs the native
   image. Making sync build an image would put a cross-cutting compile behind
   a command run 22 times per fixture build (issue 0641). Sync does one thing:
   after `nros ws entity-inventory`, if a census exists for the entry it
   reports whether it is fresh, in the same "sync: source metadata" block that
   today reports "no producer".
3. **The RTOS image's configure requires a fresh census.** `nano_ros_entry`
   for a non-`native` DEPLOY already runs `nros_record_entity_facts` and the
   inventory at configure time; it gains a call to `entity-census check
   --require-fresh`. Policy in `system.toml`, `[census] on_missing = "warn" |
   "refuse"` and `on_stale` likewise. The phase lands with `warn` as the
   default so no consumer is broken on the day it merges; W6 flips the island
   to `refuse`, and the default follows once two consumers have run under it.
   A refusal names the command that produces a fresh census; it never tries
   to build the native image from inside a cross configure.

Staleness is content-addressed, never mtime-based - the rule
`metadata_refresh.rs` states and the reason it states it (the fixture mtime
treadmill). The census records `inputs`: the digest of every component
package's source tree (the `source_digest` walk the sidecar refresh already
uses), the generated entry TU, the model's `meta.inputs` digests, the nros-cpp
crate version and the recorder schema version. Fresh means every recorded
digest equals the current one; a changed recorder schema is stale by
definition (issue 0427's rule for the resolver pin, applied here). The census
is written atomically beside the model (issue 0498).

Acceptance: edit one component source (add a subscription) and the RTOS
configure refuses with "census stale: autoware_mrm_handler changed since
<digest>"; re-run the census and it refuses with `missing-in-contract`; add
the contract row and it configures. Touch a file without changing it and
nothing is stale.

Claim: phase-463-W4. Depends on: phase-463-W3, phase-460-W1. Owns: the entity-census recipe in just/check/codegen.just (new), packages/cli/nros-cli-core/src/cmd/ws.rs (the sync: source metadata block), packages/cli/nros-cli-core/src/orchestration/metadata_refresh.rs, cmake/NanoRosEntry.cmake (the --require-fresh call), the [census] policy keys in cargo_metadata_schema.rs. Gate: just check entity-census (new). Status: not started.

### W5 - the compatibility invariants, as gates

The constraint is that the C++ API stays identical between native and RTOS
and that the RTOS image is byte-for-byte unaffected when analysis is off.
Four invariants, each with a gate and a negative control:

| invariant | gate | negative control |
| --- | --- | --- |
| I1: no user-code change, no ifdef | the census binary IS the native entry: same target, same object files, no second compile. Gate: the census run's `nm` output equals the boot binary's | a build that passes `-DNROS_CENSUS` to component TUs is a gate failure by construction, because there is no such flag to pass |
| I2: identical C++ surface | `check-api-parity` (already in the default check) plus a grep over `packages/api/nros-cpp/include/nros/*.hpp` and `nros_cpp_ffi.h`: no declaration is conditional on `metadata-mode` / `profile-mode` except the two dump exports that already are | add a `#ifdef NROS_METADATA_MODE` around any public declaration and the grep fails |
| I3: zero bytes in the RTOS image when off | (a) the resolved feature set of every non-native umbrella excludes `metadata-mode` and `profile-mode`, read from the cargo invocation the way `check-knob-delivery` reads knobs; (b) `nm` of the RTOS staticlib contains neither `nros_cpp_metadata_dump` nor any `nros_rmw_metadata` symbol; (c) the image-facts lane's `.text/.data/.bss` for the reference Zephyr image are unchanged by this phase to the byte | force the feature on for one RTOS build and all three fail; revert and they pass |
| I4: the hooks cost nothing when off | the hook bodies are `#[cfg(feature)]` and the functions are `#[inline]` with empty bodies otherwise; I3(c) is the measurement | same as I3 |

I3(c) is the one that matters and the one that cannot be argued from
structure: it is measured, and this document records the two numbers.

Claim: phase-463-W5. Depends on: phase-463-W2 (I1, I3b, I3c); none for I2 and I3a. Owns: census-no-conditional-api in just/check/abi.just (new), rtos-feature-set-excludes-analysis in just/check/platform.just (new), packages/api/nros-cpp/src/metadata_hooks.rs (inline empty bodies), the I3(c) numbers in this document. Gate: the two new recipes, just check api-parity, the image-facts lane. Status: not started.

### W6 - retire the max, flip the island

With a census that reports disagreements, `count_callbacks_with_recorded`'s
`max(modelled, recorded)` is a rule that hides what W3 reports; it is
replaced by the census verdict, and a bake with a fresh passing census reads
the count from the inventory alone (it already does on every road where the
inventory reaches the executor; the max was only ever load-bearing where no
inventory existed). The `nros::main!` macro keeps its sidecar reader for leaf
packages.

The island flips `[census] on_missing = "refuse"`, `on_stale = "refuse"`, and
deletes the `NROS_EXECUTOR_MAX_CBS=32` export from its `just build`, because
the reason for it (a native image that must not die at boot on a count the
build could not check) is gone. The runtime refusals `DECLARED_DEPTH_MISMATCH`
(-403) and `DECLARED_PARAM_MISMATCH` (-446) STAY on the RTOS: they are the
defence for a call site the census could not key (a runtime-built topic
name), they cost what they cost today, and a check that runs one build
earlier does not make the later one wrong.

The island half (the `[census]` flip to `refuse` and the deleted
`NROS_EXECUTOR_MAX_CBS=32` export) is a separate unit, `island-W2`, in the
island's own phase doc; the nano-ros half above is what this claim covers.

Claim: phase-463-W6. Depends on: phase-463-W3, phase-463-W4. Owns: packages/core/nros-orchestration-ir/src/executor_sizing.rs, packages/cli/nros-cli-core/src/orchestration/model_ingest.rs (:406); the island half is island-W2 in the island's own phase doc. Gate: cargo test -p nros-orchestration-ir and just check entity-census on the island. Status: not started.

### W7 - the profiling half (later, and a separate decision)

The census answers "what does the code create". Profiling answers "what does
it cost", and on the host it answers that for the host. The wave is designed
here so that its boundary is written down before anyone measures the wrong
thing.

**What host timing CAN say about the RTOS image.** Structure and counts
transfer, because they are properties of the code and the launch, not of the
CPU: which callbacks exist and their periods (the census); which outputs a
timer tick actually publishes, which is the `paths.<p>.output` list the
contract declares and today's layer map marks as TRUSTED; the callback ->
publish edges, which are the causal graph the rate hierarchy and the tier
derivation reason over; the serialized size of every message actually sent,
which checks the bound inventory against a real payload; and the number of
arena allocations per callback under the same allocator, which is a property
of the code path.

**What it CANNOT say.** Execution time. A Cortex-M7 at 160 MHz with a
different cache, a different FPU, no branch predictor to speak of and an ISR
load the host does not have shares no cycle count with an x86-64 host, and
RFC-0078 D1 says why in one line: a WCET belongs to a context, not to code.
So the host profile populates a NAMED profile and nothing else:

```toml
[wcet.profiles.host-x86_64-release]
cpu      = "x86_64"
clock_hz = 0          # unknown by design: a host clock is not a fact of the image
profile  = "release"
```

and the Zephyr board's `[image.zephyr]` never selects it. The native target
MAY select it, which gives the native image's feasibility check a measured
number where it has `None` today (issue 0259's absent-is-not-zero warning
would then name only the RTOS profile as absent, which is the truth).

**What is written.** `nros.wcet.measurements/1` per issue 0403, one row per
callback with `max_observed_cycles` (here: nanoseconds, with the unit stated),
`coverage` (which launch, which inputs, how many ticks), and no `bound_cycles`:
RFC-0078 D1b, a measured maximum is evidence and converts to nothing without a
written `margin_percent`. The profiling hook is in the executor's dispatch
(`spin.rs`, where the callback is invoked) behind a `profile-mode` feature the
native umbrella may enable and no RTOS umbrella does; W5's I3 covers it with
the same three gates, and the same negative control. Output goes to
`NROS_PROFILE_OUT=<path>`, the same environment-only switch as the census,
for the same reason.

Acceptance: the island's native image, run under a replayed input set for 60
seconds, produces a measurements file whose callback rows match the census
one-to-one, whose observed `paths` outputs equal the contract's `output:`
lists for all four timers, and whose numbers are refused by the Zephyr bake
if anyone tries to select the host profile for it.

Claim: phase-463-W7. Depends on: phase-463-W2, phase-463-W5, and the separate go/no-go decision. Owns: packages/core/nros-node/src/executor/spin.rs (the dispatch hook), profile-mode in packages/core/nros-node/Cargo.toml, packages/api/nros/Cargo.toml and packages/api/nros-cpp/Cargo.toml, the NROS_PROFILE_OUT switch in packages/api/nros-cpp/src/lib.rs, docs/design/0078-wcet-is-declared-per-profile.md. Gate: the 60 s replay on the island's native image. Status: not started.

## Gates added by this phase

| gate | what it asserts | wave |
| --- | --- | --- |
| `check-entity-census` | census fresh and check passes `--strict` on every fixture workspace with a bringup | W4 |
| `check-census-hooks-complete` | every `nros_cpp_*_create` / `nros_cpp_node_declare_param_*` entry point calls its hook; fixture negative control | W1 |
| `check-census-no-conditional-api` | no public header declaration is feature-conditional beyond the two dump exports | W5 |
| `check-rtos-feature-set-excludes-analysis` | no non-native umbrella resolves `metadata-mode` or `profile-mode`; `nm` finds no analysis symbol in an RTOS staticlib | W5 |
| the image-facts lane | reference Zephyr image sizes unchanged to the byte | W5 |
| the phase-308 layer grep | no JSON, schema struct or slot arithmetic in the hooks or the backend | W1 |

## Limits

* **A conditional entity is a lie the census cannot see.** If a node creates
  a subscription only under some parameter value, the census sees one
  configuration. The contract cannot express that either, so the census and
  the contract agree on the same lie. Not this phase's to fix; it is the
  contract's expressiveness.
* **A runtime-built topic name is keyed at runtime.** The census records what
  was created, so it sees the resolved name; the compile-time `static_assert`
  cannot, which is why -403 stays.
* **C entries.** The C ABI (`nros-c`) has no hooks; the C++ umbrella bundles
  it, so a C node's entities crossing `nros_publisher_create` are visible to
  the recording backend but not attributed to a node (no cursor) and its
  timers are not seen. Rust nodes take phase-307's own producer. The census
  covers C++ first because that is what the reference consumer is written in.
* **One node per class instance is assumed by nothing** - the cursor keys by
  FQN - but two instances of one class with different launch parameters are
  two census rows, and the check compares each against its own contract entry.
* **The census is not a test of behaviour.** It runs constructors. A node
  whose constructor blocks on discovery cannot be censused, and says so
  (issue 0286's shape); the `unobserved` verdict is the honest result.

## Composition with the declared-QoS and declared-params checks

| check | keyed by | runs | sees | cannot see |
| --- | --- | --- | --- | --- |
| `NROS_ASSERT_DECLARED_DEPTH` (phase-403 step 2) | the code's call site, compile time | every RTOS and native compile | a call site whose depth disagrees with a declared row | a declared row with no call site; a call site with no declared row |
| `check_declared_depth` / -403 | the code's call site, boot | every boot | the same, for runtime-built names | the same |
| `check_declared_param` / -446 (phase-446 W6) | the code's `declare_parameter`, boot | every boot | a name or type the contract lacks | a contract parameter the code never declares |
| the census check (this phase) | the contract row AND the code's created entity, host | before the RTOS configure | every row of W3's table, both directions | a conditional entity (see Limits) |

They are not redundant and they do not replace each other. The first three are
keyed by the code and catch the code disagreeing with a declaration it can
see; the census is keyed by the join and catches absence in either direction,
which no check keyed on one side can. When the census is fresh and green, the
first three are expected to pass; when one of them fails on a green census,
that is a census defect and a bug against this phase.

## Open questions

1. **Should the census be allowed to SUPPLY a contract skeleton?**
   `entity-census emit-contract` would write the `nodes:` block from the
   census for a workspace with no contract yet. phase-403 W9's rule is that
   the declaration supplies and evidence verifies, for a reason of build-graph
   direction; a skeleton emitted for a human to edit does not violate it, but
   a skeleton that is committed unread is the same bug as E3b with a
   different author. Proposed: emit with every row marked `# from census
   <date>, review`, and refuse to emit over an existing file.
2. **`unwired` belongs to the resolver.** An endpoint under `sub:` with no
   `topics:` row is a contract defect the rlm checker should refuse, upstream
   of nano-ros. This phase checks it in W3 because the island needs it now;
   the upstream rule is filed against play_launch and, when it lands, W3's
   row becomes a duplicate to retire.
3. **Is `phantom` an error?** It is the safe direction for memory. It is
   also how E3b's over-provision was never found. The default here is error
   with a per-row waiver; the alternative (warning by default, `--strict` in
   CI) makes the merge queue the only place the truth is enforced. Decide at
   W3 with two consumers' experience, not before.
4. **A timer period read from a parameter changes at runtime.** Parameter
   services can set `rate` after the census ran; the contract's `rate_hz` is
   then wrong at runtime and no host check can know. Either the contract
   marks a rate as parameter-derived (rlm schema) or the node refuses the set
   (a `read_only` descriptor, which phase-446 W6 can check). Out of scope,
   recorded.
5. **The probe's E0428 collision** (one FFI glue crate per package, colliding
   on shared interface types in the batch project) is a phase-313 defect
   independent of this phase. W2 makes the island stop needing the probe;
   leaf packages still do. File separately when someone has a leaf that hits
   it; do not fix it under this phase's number.
6. **Where does the census live for a workspace with several entries?** One
   file per entry, and the check compares each entry's census against the
   model that entry was generated from. Two entries sharing a component see
   it twice; that is correct, they may launch it with different parameters.

## Docs to update when waves land

* `docs/roadmap/phase-403-type-bound-rx-sizing.md` W9: the sentence "the
  running image verifies" gains "on the host, by the census (phase-463);
  on the RTOS, by `ExecutorFull`".
* `docs/roadmap/archived/phase-308-cpp-metadata-producer.md` and
  `archived/phase-313-*`: a pointer that the recorder's schema is v2 and the
  entry, not the probe, is the producer for bringup workspaces.
* `docs/roadmap/phase-454-contract-states-facts-backends-derive.md`: the
  descriptor (RFC-0100 D4) carries what the contract STATES; the census is the
  evidence it is true, and W12's "the descriptor carries the CONTRACT's
  facts" gains the sentence that the census check is what makes those facts
  trustworthy.
* `docs/design/0078-wcet-is-declared-per-profile.md`: an amendment naming the
  host profile convention (`host-<arch>-<profile>`) and the rule that a board
  never selects one.
* The book's contract chapter: the "two authoring modes" table for QoS gains
  the sentence that the census refuses when neither mode agrees with the code,
  and a worked refusal for each of E3a/E3b/E3c.
* The safety island (external): the contract header's 2026-09-04 incident
  note gains its fix; `nxp-deployment.md` sec. 4's "what is trusted" row for
  the `sub:`/`pub:` lists moves to "verified on the host"; the
  `NROS_EXECUTOR_MAX_CBS=32` export is deleted.
* `CLAUDE.md`: one line under the build-gates list naming
  `check-entity-census` and what a `missing-in-contract` refusal means.
