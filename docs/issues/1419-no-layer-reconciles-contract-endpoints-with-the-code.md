---
id: 1419
title: "No layer reconciles the contract's declared endpoints with what the node
  code creates; the first catch of an omitted subscription is ExecutorFull at
  boot"
status: open
type: bug
area: [build, cmake, testing]
severity: high
found: 2026-09-21
related: [0257, 0965, 1084, 0641, 0304, phase-308, phase-313, phase-403, phase-446, phase-463, rfc-0100, 1600, 1647, 1252]
---

## Problem

Since phase-412 the contract sidecar (`<bringup>/launch/<stem>.contract.yaml`)
is the sole source for every pool an image compiles with: `nros ws
entity-inventory` counts subscriptions, publishers, queryables, callback slots
and liveliness tokens from it, and the inventory header says on purpose that
it carries NO headroom, "so a stale declaration makes the image fail entity
creation with `ExecutorFull` naming this knob, rather than being absorbed
silently". The design assumes the declaration equals what the code creates.
Nothing on the host checks that it does, in either direction.

## Evidence

Measured on the safety-island consumer (four C++ nodes, one contract, native
and Zephyr images), 2026-09-20, three edits to the contract and none to the
code:

| edit | resolver | inventory | entry codegen | first catch |
| --- | --- | --- | --- | --- |
| delete `mrm_handler/operation_mode_state` and its topic (the 2026-09-04 seventh-subscription bug, replayed) | 0 errors | `MAX_SUBSCRIBERS 11 -> 10`, `EXECUTOR_MAX_CBS 19 -> 18`, `MAX_LIVELINESS 58 -> 57`, arena `50640 -> 46976` | rc=0 | `ExecutorFull` at boot, on a board with no wired console |
| declare a `phantom` sub the code never creates, wired to a topic | 0 errors | every pool one longer, arena +3664 B | rc=0 | never |
| declare the same sub under `sub:` with no `topics:` row | 0 errors | unchanged (counts come from the wiring) | rc=0 | never; the endpoint is dropped with no diagnostic |

The one host-time cross-check that exists, `NROS_ASSERT_DECLARED_DEPTH` in
`NROS_SUBSCRIBE` (phase-403 step 2), is keyed on the code's call sites and by
its own rule ("an ABSENT row is nobody declared this endpoint, and nothing
asserts against it") cannot see an endpoint the code never mentions or a
call site the contract never mentions. The declared-params check (phase-446
W6, `DECLARED_PARAM_MISMATCH -446`) has the same one-sided shape.

## Why the existing recorder does not cover it

phase-308 built the entity recorder (`metadata-mode`: a recording RMW
backend plus three executor-side hooks in `packages/api/nros-cpp/src/metadata_hooks.rs`)
and phase-313 the C/C++ probe that runs it from `nros sync`. On the island,
today:

* the probe FAILS for all four components (`error[E0428]:
  builtin_interfaces_msg_duration_t defined multiple times` - the batch
  project's per-package FFI glue crates collide on a shared interface type),
  issue 0641's negative cache stops retrying, and sync continues with "no
  producer", which is its documented fallback;
* the two sidecars that survive from 2026-09-11 record `depth: 10` for a
  subscription the code creates at `::nros::QoS(1)`, because
  `nros-rmw-metadata::create_subscriber` takes `_qos` and never reads it
  (`packages/rmw/metadata/src/lib.rs:131`);
* `parameters: []` in both, because no hook observes
  `nros_cpp_node_declare_param_*`;
* the only consumer of a recorded count is
  `count_callbacks_with_recorded` (`packages/core/nros-orchestration-ir/src/executor_sizing.rs:154`),
  which takes `max(modelled, recorded)` for one number and therefore cannot
  report a disagreement; `packages/cli/nros-cli-core/src/entity_inventory.rs`
  never reads a recorded sidecar at all.

So the seam that could reconcile exists, is incomplete, is broken on the
reference consumer, and feeds nothing that compares.

## Current state

Open. The island's native `just build` exports `NROS_EXECUTOR_MAX_CBS=32`,
so its native image is hand over-provisioned and would not have shown the
omission even at boot; the Zephyr image (derived 19) would have, on silicon
with no console.

### 2026-10-01 -- the census reaches a real cross configure

phase-463 W1-W4 landed (the recorder, the native entry as producer, the
verdict check, the configure-time `--require-fresh` call -- the last as PR
#1233), each gated on a FIXTURE. Run end to end on an in-tree workspace for
the first time, the road had three defects, each of which on its own meant no
cross configure ever compared anything:

1. **The C++ entry the workspaces generate produced no census.** The typed
   single-executor C++ native entry calls the header-only
   `nros::board::LinuxBoard::run_components`, which never reached the Rust
   funnel W2 put the `$NROS_CENSUS_OUT` switch in. Measured on
   `examples/workspaces/cpp` `native_entry`: the run ignored the variable,
   dialled zenoh and exited 156 on `ConnectionFailed`, writing nothing.
2. **The producer and the consumer named different files.** Both sides keyed
   the census by `<their own build dir>/nros/census/<their own entry>.json`,
   so the native run wrote `.../native_entry.json` and the threadx configure
   looked for `build/threadx-linux-zenoh/cmake/nros/census/threadx_entry.json`
   -- and its remedy said `nros ws entity-census run --entry threadx_entry`,
   an entry with no host binary. Every cross configure read "census missing",
   which the landing `[census] on_missing = "warn"` lets build.
3. **`run` could not find a binary `nros build` had built**
   (`build/<image-root>/cmake/<entry>`), so the documented remedy refused.

Fixed in the PR that carries this section (*the census reaches a real cross
configure*): `LinuxBoard::run_components` calls `nros_cpp_census_begin` /
`nros_cpp_census_finish` (the funnel's own three functions, no second census
path); the census of a MODEL lives beside it
(`<model-dir>/<stem>.census.json`, `census_path_for_model`), which is the one
document both images of one launch file share, and cmake ASKS for that path
(`nros ws entity-census path --model`) instead of spelling one; `run` searches
`nros build`'s image roots and refuses an ambiguity by name.

Measured on `examples/workspaces/cpp` (native image + `threadx-linux` cross
configure, census taken by `native_entry` against the pristine contract):

| edit to `system.contract.yaml` | threadx configure, before | after |
| --- | --- | --- |
| none | `census missing (WARNING)`, builds | `3 confirmed, 0 error(s)`, builds |
| E3a: drop `listener.sub.chatter` + its wiring | `census missing (WARNING)`, builds | REFUSED: `error missing-in-contract listener sub /chatter`, not waivable, remedy names the two lines |
| E3b: declare + wire `listener.sub.phantom` | `census missing (WARNING)`, builds | REFUSED: `error phantom listener sub /phantom`, waiver key printed |
| E3c: declare `listener.sub.phantom`, no `topics:` row | `census missing (WARNING)`, builds | REFUSED: `error unwired listener sub phantom` |

**The census producer is sized by the contract it checks.** That is the
fourth defect, and it is the one that makes E3a reach the census run itself
when the native image is REBUILT from the edited contract: the native image's
callback table derives one short and the census run stops at `ExecutorFull`
(measured: exit 250, `2 node(s), 0 sub / 1 pub / ... / 1 timer`). Before, it
discarded what it had and the configure read "census missing" (warn). Now the
run writes what the recorder saw, marked `incomplete`, exits non-zero, and
every check against it refuses (`census INCOMPLETE`); `nros sync` reports it
as "current but INCOMPLETE". Measured: the threadx configure REFUSES. It does
not name the omitted row -- the executor refuses a registration
(`next_entry_slot`, 14 sites in `nros-node`) before the recorder sees it.

Gates: `check-entity-census` now never names the census path on either side,
takes it with one entry name and checks it with another (the real road), and
gains move 4b (an incomplete census is written and refused);
`workspace_metadata::cmake_cpp_workspace_entry_writes_a_census_without_a_router`
runs the PREBUILT C++ workspace fixture in census mode (negative control
measured: with the header change reverted it fails, exit 156, no file).

### 2026-10-02 -- the census is not sized by the contract it checks

Fixed in the PR that carries this section (*the census is not sized by the
contract it checks*). All measured on copies of `examples/workspaces/{cpp,c}`
with no router running; none of the census runs needs one.

**Issue 1600's workaround is gone.** With PR #1557 (the multi-entry fragment
is the union of every entry) on main, the pristine `examples/workspaces/cpp`
`native_entry` census exits 0 with `1 sub / 1 pub / 1 timer` and no
`NROS_EXECUTOR_MAX_CBS` override. That union is also why E3a, rebuilt from the
edited contract, now names its row on THIS workspace -- the action and service
entries' callbacks give `native_entry` headroom. That is luck, not structure:
a single-entry image (the island's) has none.

**E3a names its row whatever the image's sizing** (item 2 above, done). The
census run opens its executor at the executor's own ceilings -- 64 callback
slots (the `u64` ready-set bitmask), 64 nodes, a 16 MiB `MaybeUninit` arena,
leaked from the host heap -- instead of at the contract-derived `MAX_CBS`
(`CENSUS_SIZING` in `nros-cpp`, armed by `census_select_backend` and consumed
by the one executor open that follows, so a normal boot of the same image keeps
the build's sizing). Measured with the native image built at
`NROS_EXECUTOR_MAX_CBS=1` from the E3a contract:

| | census run | threadx configure |
| --- | --- | --- |
| before | `ExecutorFull`, exit 250, `0 sub / 1 pub / 1 timer`, INCOMPLETE | REFUSED: `census INCOMPLETE`, no row named |
| after | exit 0, `1 sub / 1 pub / 1 timer` | REFUSED: `error missing-in-contract listener sub /chatter`, remedy names the two lines |

Test: `census_funnel_tests::census_is_not_sized_by_the_contract_it_checks`
(`MAX_CBS + 3` timers through the real funnel; red before -- "7 callbacks
against a build sized for 4 must record every one, not stop at ExecutorFull"
-- green after), whose second half is the negative control: the same setup on
the same funnel with no `$NROS_CENSUS_OUT` still stops at the build's
`MAX_CBS`.

**A model with no contract has nothing to reconcile** (found here).
`examples/workspaces/c` has no `*.contract.yaml`; its threadx configure warned
"census missing" with no census and FAILED the moment one was taken (`no
--contract, and the model names no *.contract.yaml`) -- producing the evidence
broke the build. The check now says `no contract ... nothing for a census to
reconcile` and passes, with or without a census, under either policy (gate
move 6, red on the old CLI). Measured: the C threadx configure builds with the
census present.

**`warn` is loud now, and stays the default** (item 1: measured, not flipped).
Every cross image that reaches the check today is a C or C++ entry and CAN
produce a census, but nothing that builds them unattended takes one first:
`build-test-fixtures` configures the `examples/workspaces/cpp` cross rows
(`freertos_posix`, `s32z270`, `mps3_an536`, `threadx`, ...) with no native
census run before them, so `refuse` would fail those builds for a reason the
build cannot fix. What changed is that the warning is a **CMake WARNING**
naming this issue, saying that the image's pools come from a contract nothing
compared with the code, and naming `[census] on_missing = "refuse"` -- not a
`STATUS` line among hundreds (gate move 5 asserts both texts).

**C entries are census producers** (item 3, half). Measured on
`examples/workspaces/c`: `native_entry` writes `talker` (1 publisher, 1 wall
timer) and `listener` (1 subscription), attributed per node. The generated C
entry runs through `nros_board_native_run_components_named_in` (issue 1597),
creates each node with `nros_cpp_node_create` (which opens the recorder's node
cursor) before configuring its component, and a C component creates its
entities through the same `nros_cpp_*` ABI the hooks sit on. phase-463's
"Limits" note was about a C node that opens its own node through `nros-c`;
that case is still unattributed. Pinned by
`workspace_metadata::cmake_c_workspace_entry_writes_a_census_without_a_router`
beside the C++ one, which now asserts a COMPLETE census (exit 0) instead of
tolerating issue 1600's exit 250.

**phase-463 W5, two of four invariants as gates.** `check-census-no-conditional-api`
(I2: no tracked `nros-cpp` header has a preprocessor conditional naming the
metadata / profile / census modes, and the census pair stays declared; a
planted `#ifdef NROS_METADATA_MODE` in `main.hpp` measured red) and
`check-rtos-feature-set-excludes-analysis` (I3a: asks `nros_feature_set` for
all 32 crate x platform x cross sets and holds the OUTPUT to `metadata-mode`
iff cpp + posix + native, `profile-mode` never; widening the guard to
`if(_FS_CRATE STREQUAL "cpp")` measured red). Both carry an on-every-run
negative control.

### 2026-10-03 -- the default is `refuse`

Fixed in the PR that carries this section (*the census default is refuse*).
Measured on copies of nothing -- the in-tree `examples/workspaces/cpp` and
`derived-tiers-cpp`, no router running.

**`nros ws entity-census take --image <bringup>:<image>`** is the one command a
cross build runs first. It resolves the image's model, answers "nothing to
take" for a host image, a model with no contract or a census that is already
fresh, and otherwise finds the HOST sibling of the same bringup resolved from
the same launch and arguments (board platform `posix`, first by id), builds it
with `nros build`, and runs it in census mode. The configure's refusal now
prints exactly that command for the image asking (`take --image
demo_bringup:threadx`), and `nros sync`'s stale-census line names it too.

**Three defects stood between the census and the flip, each measured:**

1. **The native image was never the same file twice.** `nano_ros_link_rmw`
   rewrote `nros_app_register_backends.c` with `file(WRITE)` on every
   configure, and `nros build` configures on every run, so every rebuild
   recompiled it and relinked all seven native entries of
   `examples/workspaces/cpp` with no input changed (14 compile/link lines per
   no-op build). The census's `binary` freshness input therefore went stale on
   every rebuild of the image that produced it -- measured: census taken,
   `nros build demo_bringup:native` again, the threadx configure read `census
   stale: binary ... changed`. Write-if-changed now (and the identical PX4
   module stub): a no-op `nros build` compiles and links nothing, the binary
   keeps its digest, and the census stays fresh across the native fixture
   rows' own rebuilds (measured: `native_robot1` and `native` rebuilt after a
   take, `take` still answers `fresh`).
2. **A clean first build is not the fixed point** (filed as issue 1647). From
   a clean build dir the first `nros build` links `native_entry` with the
   message-bound knobs at their placeholders and the second changes its bytes;
   builds 2, 3 and 4 are identical. `take` builds until two consecutive
   binaries agree (ceiling 3), so a census is never of a build the next one
   will replace.
3. **The island-shaped workspace could not produce a census at all.**
   `examples/workspaces/derived-tiers-cpp`'s native image is a single-entry
   configure, so its sizing descriptor reaches cargo and says `backend_count
   = 1`; the RMW registry then held exactly zenoh's slot, the recorder's
   registration failed silently (`let _ =`), and the census run died at
   backend selection: `$NROS_RMW names a backend that is not registered`, exit
   253, no file. `nros-rmw-metadata` now enables `nros-rmw-cffi`'s
   `recorder-slot`, which adds the recorder's slot BESIDE the declared
   backends, and a failing registration says so. Measured after: `4 node(s),
   0 sub / 4 pub / 4 timer slot(s)`, and the check against its contract reads
   `8 confirmed, 0 error(s)`. Test:
   `census_funnel_tests::the_recorder_has_a_registry_slot_of_its_own` (red
   with the feature removed from `nros-rmw-metadata`, measured).

**Unattended builds take the census first.** `scripts/build/census-prepass.sh`
enumerates every cross workspace image in scope (`fixtures-manifest.py
census-images`: a `[[workspace_fixture]]` row with an `image`, not `linux`, not
pure Rust) and runs `take` for each. `build-test-fixtures` runs it ONCE, serially,
before the platform stages start in parallel, and exports
`NROS_CENSUS_PREPASS=done`; `workspace-fixtures-build.sh` and the Zephyr leaf
lane run it themselves only when invoked directly. The order is the point:
`take` builds a workspace's native image into the tree the native stage builds
into, so a take inside a cross stage would race that stage.

**The flip.** `[census] on_missing` / `on_stale` default to `refuse`; `warn`
is the explicit opt-out and stays a CMake WARNING. Measured on
`examples/workspaces/cpp` through the fixture builder
(`workspace-fixtures-build.sh threadx-linux cpp`), census file deleted first:

| | threadx configure |
| --- | --- |
| `NROS_CENSUS_PREPASS=done` (no census taken) | REFUSED: `census missing`, remedy `nros ws entity-census take --image demo_bringup:threadx` |
| the builder's own pre-pass | `take` builds `demo_bringup:native` and runs it; `census check: 3 confirmed, 0 error(s)`; `threadx_entry` built |

Every in-tree configure that reaches the check is one of the two contracted
C++ workspaces' cross images (`cpp`: `freertos`, `freertos_posix`, `s32z270`,
`mps3_an536`, `threadx`, `zephyr`, `zephyr_cyclonedds`; `derived-tiers-cpp`:
`zephyr`), all built through the three callers above, and both workspaces'
censuses were measured taking and passing. NOT measured: a Zephyr configure
itself (the only Zephyr workspace on this host belongs to another checkout,
and building through it would measure that tree -- issue 1280); the check
there is the same cmake function.

Gates: `check-entity-census` move 0 asserts the remedy names `take` for the
image asking, and move 5 now asserts that a bringup with NO `[census]` refuses
and that `warn` is the loud opt-out (the main checkout's CLI measured red on
move 0). Unit tests pin the remedy and the sibling choice (by model and by
platform, first by id, same bringup only).

**phase-463 W5, measured by hand on the images above** (not gated -- they need
built images): I3(b) holds -- the threadx-linux `libnros_cpp.a` defines no
`nros_cpp_metadata_dump` and no `nros_rmw_metadata*` symbol (the native one
defines 1 and 97), and `threadx_entry` links none (`native_entry`: 102). I1
holds by construction: `take` runs the binary `nros build` wrote, at the path
the boot runs. I3(c) (the image-facts bytes of the reference Zephyr image) is
not measured.

### What is left (why this stays open)

* **phase-463 W6** -- retiring `count_callbacks_with_recorded`'s
  `max(model, recorded)` in favour of the census verdict -- is not started.
  Its one CLI caller is `codegen-system`'s capacity check
  (`model_ingest::check_executor_capacity`), which has no census or inventory
  in hand; the island half (`island-W2`) is external.
* **Rust entries have no census, and the cargo road has no consumer.**
  `boot_hosted` refuses `$NROS_CENSUS_OUT` for a Rust entry (the hooks are on
  the C++ ABI; a Rust node's timers and node identity never cross it), and the
  freshness check is called from `cmake/NanoRosEntry.cmake` only, so a Rust
  RTOS image built by cargo is never checked even when its workspace has a
  contract (`examples/workspaces/realtime-rust` does). Both halves are open.
* **A C node that opens its own node through `nros-c`** reaches the recording
  backend with no node attribution. No in-tree C workspace does this.
* **phase-463 W5 I1, I3(b), I3(c) are not GATED**: I1 holds by construction
  and I3(b) was measured by hand again above; I3(c), the image-facts byte
  comparison for the reference Zephyr image, is not measured. All three need
  built images, so they belong in a build-tier lane.
* **What still stops a census run** is code that boots at no sizing: more than
  64 callbacks in one executor, a constructor's own error, or the parameter
  store (`CENSUS_SIZING` resizes the executor only). Those still produce an
  INCOMPLETE census, which every check refuses.

## Fix / direction

[phase-463](../roadmap/phase-463-host-census-reconciles-contract-with-code.md):
complete the recorder (QoS as passed, a parameter hook, timer kinds), make the
generated native ENTRY the census producer (same binary as boot, switched by
`NROS_CENSUS_OUT` in the hosted funnel, where the `env` capability already
lives and no RTOS board has one), and add `nros ws entity-census check`
emitting one verdict per (node, kind, name) - `missing-in-contract`,
`phantom`, `unwired`, `depth-mismatch`, `type-mismatch`, `period-mismatch`,
`param-*` - with the UNDER direction an unwaivable error and the OVER
direction waivable per row. The RTOS configure requires a fresh census
(content-addressed, RFC-0063-shaped provenance). Nothing is added to the RTOS
image: the feature is on the native umbrella only, and phase-463 W5 gates that
with a feature-set check, an `nm` check and the image-facts byte comparison.

Acceptance for closing this issue is phase-463 W3's: the three edits above
each refuse on the host with one named row, before any RTOS image is
configured.

## Revised direction for the two Rust items (2026-10-03, RFC-0100 Amendment 1)

Re-read against the unified build path (RFC-0065 `nros build`, phase-470's
generated entries, phase-474's one entry lowering):

* **The Rust census producer — move the hooks, do not add a Rust copy.** The
  hook bodies sit in `nros-cpp`'s `metadata_hooks.rs`; a Rust node's API is the
  `nros` crate, and `nros-c` and `nros-cpp` both depend on `nros`. Moving the
  bodies to `nros` (behind `metadata-mode`, calls unconditional) gives a Rust
  node's node / timer / parameter creation the same hooks, and is also issue
  1556's item 1 (a C node opening its node through `nros-c`). Then
  `boot_hosted` honours `$NROS_CENSUS_OUT` for a Rust entry instead of refusing
  it; the generated Rust entry is the `nros::main!` parity rendering
  (RFC-0091 §7), so the switch belongs in the hosted arm `nros::main!` expands
  to, not in an emitter.
* **The cargo-road consumer — `nros build` stage 4, not a build script.** That
  stage already resolves a cargo image's model and writes its sizing descriptor
  (`cmd::build`), so it is the one place on the cargo road that holds both the
  model and the image before any compile. A `build.rs` that refuses on a census
  file would be a fourth reader of the census path and would fire inside every
  incremental `cargo build`. The cmake configure keeps its check, because the
  generated cmake root is a real build tree that may be rebuilt by hand.
* **The census is per MODEL, which is what RFC-0100 D12 composes over** — so in
  an N:1 cmake configure (several images sharing one runtime) the runtime's
  descriptor is trustworthy exactly when each model's census is fresh. Nothing
  to change here; recorded so the two are not designed apart.

Files: the hook move — `packages/api/nros/`, `packages/api/nros-cpp/src/metadata_hooks.rs`,
`packages/api/nros-c/src/`, `packages/boards/nros-board-linux/src/lib.rs`
(`boot_hosted`); the consumer — `packages/cli/nros-cli-core/src/cmd/build.rs`
(the cargo stage-4 block) and `cmd/entity_census.rs`. **Overlap warning:**
`cmd/build.rs` is also where issue 1647's fix would make the first build the
fixed point — coordinate with whoever holds 1647.
