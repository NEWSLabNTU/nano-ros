# phase-460 - every consumer of the contract chain verifies what it consumes

**Status (2026-09-21). PROPOSED; nothing landed.** Numbered 460 because
phases 456-458 were being opened concurrently by other sessions; this is
highest-existing (455) + 5. Sibling of phase-459 (the tier derivation a C++
image cannot reach); the two came out of one investigation of the Autoware
Safety Island chain (briefs B and D, 2026-09-18, re-verified against
`783cdfa14` on 2026-09-21).

## Parallel plan

Seven waves, independent by design: no wave's gate needs another wave's code.
One claim id per wave (`just claim phase-460-Wk`; advisory, TTL in hours, an
open PR supersedes it). W2, W4 and W6 are one-day waves a session may take in
one sitting, claiming each id. W5 and W7 both write the boot-report record and
the exhaustion path in `platform.c`, so W7 lands after W5 or the same session
holds both. Every path below exists in the tree today except the ones marked
new.

| claim id | depends on | owns | gate | starts now? |
| --- | --- | --- | --- | --- |
| `phase-460-W1` | none | `packages/cli/nros-cli-core/src/cmd/ws.rs` (the `run_sync` refusal path; `model_provenance_stale` moves out), `packages/cli/nros-cli-core/src/cmd/model_path.rs`, `packages/cli/nros-cli-core/src/model_gate.rs` (new), one gate call each in `packages/cli/nros-cli-core/src/cmd/entity_inventory.rs`, `cmd/codegen_system.rs` and `cmd/codegen.rs`, `packages/api/nros/src/lib.rs` (`load_for_build_script`), `docs/design/0063-system-model-is-a-build-artifact.md` | the refused-resolve fixture test in `packages/cli/nros-cli-core`, run by `just ci-l1` | yes |
| `phase-460-W2` | none | `cmake/NanoRosEntityFacts.cmake` (the `refused` branch at :622), `packages/core/nros-params/build.rs` (the `default` fallback at :169-170), `tests/cmake-entity-inventory-tests.sh` (one case per status) | `tests/cmake-entity-inventory-tests.sh`: `refused` fails the configure naming the node; `absent` and `declared` pass | yes (one day) |
| `phase-460-W3` | none | `zephyr/Kconfig` (the `NROS_FRAG_MAX_SIZE` entry and its `-1` sentinel), `zephyr/cmake/nros_cargo_build.cmake` (the rx-ceiling block, :780-870), `packages/cli/rosidl-lower/src/lowered.rs`, `packages/core/nros-serdes/src/size.rs`, `packages/cli/nros-cli-core/src/entity_inventory.rs` (the `NROS_ENTITY_PLAIN_TYPES` carrier), `packages/core/nros-node/src/executor/arena.rs` (the borrowed-view dispatch selection) | issue 1368's acceptance plus the unit test that a nested unbounded member is not listed, in `just ci-l1` | yes |
| `phase-460-W4` | none | `zephyr/cmake/nros_system_generate.cmake` (the compare after the bake in `nros_system_generate`), `docs/design/0049-hierarchical-platform-board-config.md`; it reads `NROS_RESOLVED_*` and adds no block to `nros_cargo_build.cmake` | `tests/cmake-domain-agreement-tests.sh` (new): `CONFIG_NROS_DOMAIN_ID=2` against a bringup declaring 10 fails the configure; equal passes | yes (one day) |
| `phase-460-W5` | none | `packages/core/nros-node/src/boot_report.rs` (`heap_peak_bytes`, `heap_capacity_bytes`), `scripts/check-boot-report-layout.py`, `scripts/read-boot-report.py`, `packages/platform/nros-platform-zephyr/src/platform.c` (the stage transitions, the report write on the exhaustion path, the stale `heap-stats` comment at :256), `just/check/tools.just` (the new recipe beside `mem-report`), `docs/design/0077-image-runtime-is-the-images-choice.md` | `just check boot-report-layout` plus the new heap-headroom recipe on a fixture dump: peak 0 refused, headroom below 24576 refused, a dump with headroom passes | yes |
| `phase-460-W6` | none | a new host test for `zephyr/nros_platform_zephyr_shims.c` under `tests/zephyr/`; the shims file itself only if a test seam is needed | the N+2 create/join test, in `just ci-l1` | yes (one day) |
| `phase-460-W7` | `phase-460-W5` (the boot-report layout and the exhaustion path in `platform.c`) | `packages/platform/nros-platform-zephyr/src/platform.c` (the exhaustion path at :212), `zephyr/Kconfig` (new `NROS_HEAP_EXHAUSTION_IS_FATAL`), `packages/api/nros-cpp/src/subscription.rs` (the take path at :606), `packages/core/nros-node/src/boot_report.rs` (`samples_dropped_too_small`), the host test in `packages/api/nros-cpp` and the native_sim test under `tests/zephyr/` | the two W7 tests; the host half in `just ci-l1` | no (yes when the same session holds W5) |

Files two waves touch, and the order they serialise in. Within this phase:
`platform.c`, `boot_report.rs`, `read-boot-report.py`,
`check-boot-report-layout.py` and RFC-0077 are W5 then W7 (W7 adds fields to
the record W5 lays out); `arena.rs` is W3 then W7 if the C++ take path has to
expose the Rust drop counter at `arena.rs:1121`; `zephyr/Kconfig` is W3, W7
and phase-461 W1 on distinct symbols in distinct menus, land order, no
dependency. Across phases: `packages/cli/nros-cli-core/src/cmd/ws.rs` is
460 W1 first, then phase-463 W2 (two dispatch lines for `ws entity-census`),
then phase-463 W4 (the census freshness line in the `sync: source metadata`
block), because W1 moves a function out of the file and 463 W4's freshness
report belongs beside the `model_gate` module W1 creates;
`cmd/model_path.rs` has one writer, 460 W1 - 463 W4's `--require-fresh`
call lives in `cmake/NanoRosEntry.cmake`, which 460 W1 does not edit
(`model-path` refusing is enough for the configure to fail).
`packages/core/nros-params/build.rs` is 460 W2 first, then phase-461 W2
(both edit the capacity-reading function; W2 here is one day, and 461 W2
depends on it). `packages/cli/nros-cli-core/src/entity_inventory.rs` is
460 W3, then 461 W3, then 461 W6; phase-463 reads the inventory JSON and owns
no line of it. `zephyr/cmake/nros_cargo_build.cmake` is 460 W3 (the rx
ceiling), then 460 W5 (the heap-gate comment), then 461 W1 (six new ladder
resolves beside :1208), then 461 W5; 460 W4 does not edit it. The regions are
disjoint, so the later wave rebases, but two claims on the file are not held
open at once. `cmd/codegen_system.rs` gets one gate call from 460 W1 while
phase-459 W1 edits `collect_callback_groups` and 459 W5 `resolve_target_block`:
disjoint functions, land order, no dependency. `packages/api/nros/src/lib.rs`
is 460 W1 (`load_for_build_script`) then 461 W6 (the :1397 assert).
Phases 457 and 462 share no file with this phase.

Owns these issues, one per wave:
[1420](../issues/archived/1420-refused-resolve-leaves-the-previous-model-for-every-consumer.md),
[1421](../issues/archived/1421-partial-params-declaration-falls-to-crate-defaults-silently.md),
[1422](../issues/1422-plain-blit-eligibility-is-computed-and-read-by-nothing.md),
[1423](../issues/archived/1423-system-toml-domain-id-and-kconfig-domain-id-are-never-compared.md),
[1424](../issues/1424-zephyr-heap-size-is-a-guess-with-a-peak-reporter-nothing-reads.md),
[1425](../issues/1425-heap-exhaustion-and-buffer-too-small-reach-no-fault-hook-on-a-consoleless-board.md).
Takes [issue 1368](../issues/1368-frag-max-size-not-checked-against-derived-bound.md)
as W3's first half. Cites [issue 1036](../issues/1036-arena-exhaustion-is-half-silent-and-wholly-unreachable.md)
(the sink problem) and [issue 1121](../issues/1121-contract-sidecar-has-no-model-freshness-edge.md)
(a different freshness edge on the same model) without owning them.

## Why

Brief B section 4 sorted every layer of the island chain into STATIC, BUILD,
RUNTIME and TRUSTED. The TRUSTED column is the finding: declared numbers that
nothing compares, computed facts that nothing reads, and one build artifact
that every consumer trusts after the producer refused to update it. Each row
below is a place where the chain's own rule - "the contract states the fact,
the build derives the number, a refusal names both" - stops one layer early.
The rows are independent; the phase exists so they are fixed as one class
(a consumer verifies its input) rather than as seven patches.

| # | trusted today | measured on the island | issue |
| --- | --- | --- | --- |
| 1 | the `system_model.yaml` on disk is the current one | E7b: resolve refused, the E6b model stayed, and `entity-inventory` / `codegen-system` / `codegen entry` derived from it (`NROS_DERIVED_MAX_PARAMETERS 26`) | 1420 |
| 2 | a partial `params:` declaration is loud | E6a: one node's block removed flips the store to `refused` and the crate defaults 32/64/256/32/256 replace 25/35/0/0/0 with no build-time line | 1421 |
| 3 | `NROS_FRAG_MAX_SIZE` and the `plain` flag mean something | 2048 hand-set, derived receive bound 1496, no comparison; `plain` computed on every bound, read by a test and a const | 1368, 1422 |
| 4 | the image's domain is the system's domain | `system.toml` writes `NROS_SYSTEM_DOMAIN_ID` into `system_config.h`; no file outside the CLI reads it; the image bakes `CONFIG_NROS_DOMAIN_ID` | 1423 |
| 5 | `CONFIG_NROS_ZEPHYR_HEAP_SIZE=94208` fits | chosen, not measured; the high-water reporter exists, is always compiled on Zephyr, and nothing reads it off the board or gates the knob against it | 1424 |
| 6 | task slots are released | RESOLVED: `zephyr/nros_platform_zephyr_shims.c:421-480` claims and releases on join (issue 0839). No issue; W6 is a gate only |
| 7 | a fault is seen | `HEAP EXHAUSTED` is a `printk` on a board with no console; a `BufferTooSmall` on the C++ take path returns `NROS_CPP_RET_FULL` with `out_len = 0` and the dispatch continues | 1425, 1036 |

## What it does

Each wave adds one verification at the consumer, one gate that fails without
it, and one negative control. Nothing here changes a derived number; the
phase changes who checks it.

### W1 - a refused resolve leaves no model to trust (issue 1420)

`nros sync` stages, stamps and renames a resolved model
(`packages/cli/nros-cli-core/src/cmd/ws.rs:2338-2348`), which is right for
the success path. On refusal the resolver exits without writing
(`packages/cli/nros-launch-resolve/src/main.rs:172` writes only after
success) and `run_sync` propagates the error at `ws.rs:2268`, so the previous
model stays in place and is intact by every check it will ever meet:
`model_provenance_stale` (`ws.rs:1448`) hashes the inputs the model RECORDS,
which are the inputs of the resolve that succeeded, not the edit that was
refused. Only `run_sync` calls it; `nros model-path`
(`packages/cli/nros-cli-core/src/cmd/model_path.rs`) checks nothing and hands
the path to `nano_ros_add_executable` (`cmake/NanoRosEntry.cmake:286-314`);
`ws entity-inventory`, `codegen-system` and `codegen entry` load whatever the
path holds.

Decision: **quarantine, then verify at every door.**

* On refusal, `run_sync` renames the previous model to
  `<model>.refused-<utc-stamp>.yaml` beside it and writes a two-line
  `<model>.refused` marker (the refusing check, the input that changed). The
  old model is kept for diffing, not deleted; the path a consumer opens no
  longer exists.
* `model_provenance_stale` moves out of `ws.rs` into a `model_gate` module
  and gains a second input: the CURRENT sha256 of every file the bringup's
  launch tree names, compared against `meta.inputs`. Every consumer that opens
  a model calls it: `model-path` (refuses with the reason, so cmake fails at
  configure, not at boot), `ws entity-inventory`, `codegen-system`,
  `codegen entry`, and `nros::main!`'s `load_for_build_script`. One gate
  function, five callers, so a sixth consumer cannot be written without it
  showing in review.
* A `.refused` marker present makes every consumer refuse until the next
  successful sync removes it, even if the inputs were edited back to the
  last-good state - a model whose producer last said no is not current.

Gate: a fixture test edits a contract to `qos: { depht: 1 }`, runs sync
(refused), and asserts that `model-path`, `entity-inventory`, `codegen-system`
and `codegen entry` all exit non-zero naming the marker; the negative control
reverts the edit, syncs, and all four pass. Runs in the fast tier
(`just ci-l1`).

Claim: phase-460-W1. Depends on: none. Owns: packages/cli/nros-cli-core/src/cmd/ws.rs, packages/cli/nros-cli-core/src/cmd/model_path.rs, packages/cli/nros-cli-core/src/model_gate.rs (new), one gate call each in packages/cli/nros-cli-core/src/cmd/entity_inventory.rs, cmd/codegen_system.rs, cmd/codegen.rs, packages/api/nros/src/lib.rs, docs/design/0063-system-model-is-a-build-artifact.md. Gate: the refused-resolve fixture test in just ci-l1. Status: not started.

### W2 - a partial `params:` declaration is a refusal, not a default (issue 1421)

`ws entity-inventory` writes `NROS_PARAM_DECLARATION_STATUS "refused"` with a
reason (`packages/cli/nros-cli-core/src/entity_inventory.rs:3993`);
`cmake/NanoRosEntityFacts.cmake:622` reads it and silently `return()`s, so
no `NROS_DECLARED_*` reaches the crate and `packages/core/nros-params/build.rs:169-170`
falls to `default` (32 slots, 64-byte names, 256/32/256 capacities). The
image builds, three times larger in its parameter store, and the
`set_parameters` request size grows with the capacities (the island's issue
1352 class).

Decision: `absent` (no node declares) keeps today's meaning - an image with
no contract is sized by its board. `refused` becomes a configure-time
`FATAL_ERROR` in `NanoRosEntityFacts.cmake` quoting
`NROS_PARAM_DECLARATION_REASON`, and the cargo road gets the same rule in
`nros-params/build.rs` when the inventory carries the status. Gate: a test
inventory with `refused` fails the configure naming the node; `absent` and
`declared` pass unchanged.

Claim: phase-460-W2. Depends on: none. Owns: cmake/NanoRosEntityFacts.cmake, packages/core/nros-params/build.rs, tests/cmake-entity-inventory-tests.sh. Gate: tests/cmake-entity-inventory-tests.sh. Status: not started.

### W3 - a stated ceiling is compared to the derived bound; a computed flag has a reader (issues 1368, 1422)

First half is issue 1368 as filed: a configure-time comparison of
`NROS_FRAG_MAX_SIZE` (`zephyr/Kconfig:867-872`, default 2048, no sentinel)
against `NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE`, refusing when the ceiling is
below the derived receive bound, and a `-1` sentinel meaning "derive it from
the bound, rounded to the reassembly granularity". Second half: the `plain`
flag (`packages/cli/rosidl-lower/src/lowered.rs:211`, "POD-blit eligible";
`packages/core/nros-serdes/src/size.rs:59`) gets one consumer or is retired.
The consumer chosen is the inventory: `NROS_ENTITY_PLAIN_TYPES` lists the
subscribed types that are blit-eligible, and the arena's borrowed-view
dispatch (`nros-node/src/executor/arena.rs`, the zero-copy tests from line
3645) is selected per type from it rather than by the current runtime
probe. If measurement shows no dispatch difference on any in-tree image, the
flag is deleted, and this wave records the measurement either way. Gate:
1368's acceptance, plus a unit test that a type with a nested unbounded
member is not listed.

Claim: phase-460-W3. Depends on: none. Owns: zephyr/Kconfig (NROS_FRAG_MAX_SIZE), zephyr/cmake/nros_cargo_build.cmake (rx-ceiling block), packages/cli/rosidl-lower/src/lowered.rs, packages/core/nros-serdes/src/size.rs, packages/cli/nros-cli-core/src/entity_inventory.rs (NROS_ENTITY_PLAIN_TYPES), packages/core/nros-node/src/executor/arena.rs. Gate: issue 1368's acceptance plus the plain-list unit test in just ci-l1. Status: not started.

### W4 - `system.toml` domain and Kconfig domain agree, or the configure says which wins (issue 1423)

`codegen-system` writes `#define NROS_SYSTEM_DOMAIN_ID <n>u`
(`packages/cli/nros-cli-core/src/cmd/codegen_system.rs:856-858`) from
`resolved_domain_id`; no file under `zephyr/`, `cmake/`, `packages/api` or
`packages/boards` reads it. The Zephyr image takes
`CONFIG_NROS_DOMAIN_ID` (`zephyr/Kconfig:1652`, default 0) through
`packages/api/nros-c/include/nros/zephyr/app_config.h:90`, and Cyclone takes
`CONFIG_NROS_CYCLONE_DOMAIN_ID` (`Kconfig:209-212`, default
`NROS_DOMAIN_ID`). The island's authored files currently agree on 10 by hand
(`system.toml` in both packages, `prj-cyclonedds.conf:102`); the brief that
opened this recorded 2 versus 10 a week earlier, and the fix was a human
noticing.

Decision: the Zephyr module's bake (`nros_system_generate.cmake`) compares
`NROS_SYSTEM_DOMAIN_ID` from the bake it just ran against
`CONFIG_NROS_DOMAIN_ID` and `CONFIG_NROS_CYCLONE_DOMAIN_ID` when set, and
refuses on disagreement naming all three. The precedence is NOT changed here:
Kconfig remains what the image bakes (RFC-0049 ladder), the check only refuses
a silent disagreement. Gate: a fixture `.config` with `CONFIG_NROS_DOMAIN_ID=2`
against a bringup declaring 10 fails the configure; equal values pass.

Claim: phase-460-W4. Depends on: none. Owns: zephyr/cmake/nros_system_generate.cmake, docs/design/0049-hierarchical-platform-board-config.md. Gate: tests/cmake-domain-agreement-tests.sh (new). Status: not started.

### W5 - the heap size is measured, and the knob is gated against the measurement (issue 1424)

What exists: `nros_zephyr_heap_peak()`
(`packages/platform/nros-platform/src/zephyr_heap.rs:103`) returns a true
high-water mark (`fetch_max` of outstanding bytes,
`packages/rmw/zenoh/zpico-alloc/src/lib.rs:264`), charged at the rlsf USABLE
size (`:321-330`), and the `stats` feature that compiles it is unconditional
on Zephyr since phase-412 (`packages/platform/nros-platform/Cargo.toml:113`).
The comment at `nros-platform-zephyr/src/platform.c:256` naming a
`heap-stats` feature is stale; the feature is `alloc-stats` on the Rust side
and `zpico-alloc/stats` on the arena. So the brief's worry (not compiled,
cumulative) is refuted by the source, and what remains is: nothing reads the
value off a board with no console, and nothing compares
`CONFIG_NROS_ZEPHYR_HEAP_SIZE` (`Kconfig:1457`) to it. The configure-time
gate at `zephyr/cmake/nros_cargo_build.cmake:911-948` checks arena + 24576
against the knob, and the 24576 is itself a measured-once constant.

Decision, the measurement: the boot report (`CONFIG_NROS_BOOT_REPORT`,
`Kconfig:1325`, a 60-byte RAM record read by `scripts/read-boot-report.py`)
gains two fields, `heap_peak_bytes` and `heap_capacity_bytes`, written at
every stage transition and on the exhaustion path, so a halted or
soft-reset board answers "how much did it need" without a console. The
gate: a `just` recipe (new, beside `mem-report` in `just/check/tools.just`)
that reads the report from a dump and refuses when
`heap_capacity - heap_peak < 24576` or when the peak is 0 (never sampled),
and a documentation rule that `CONFIG_NROS_ZEPHYR_HEAP_SIZE` in a board
`.conf` carries the dump it was set from as a comment. The island's `.conf`
already carries `CONFIG_NROS_BOOT_REPORT=y`. This wave does not run on
silicon inside nano-ros (issue 1036 records why no lane can); it ships the
fields, the reader and the recipe, and the island runs it.

Claim: phase-460-W5. Depends on: none. Owns: packages/core/nros-node/src/boot_report.rs, scripts/check-boot-report-layout.py, scripts/read-boot-report.py, packages/platform/nros-platform-zephyr/src/platform.c, just/check/tools.just, docs/design/0077-image-runtime-is-the-images-choice.md. Gate: just check boot-report-layout plus the new heap-headroom recipe on a fixture dump. Status: not started.

### W6 - the slot release survives a reconnect (gate only)

Issue 0839's fix is in the tree; no test asserts the claim/release cycle. A
host-side unit test in the shims' test build creates and joins
`NROS_ZEPHYR_MAX_THREADS + 2` tasks in sequence and asserts every create
succeeds; the negative control forces `pthread_detach` on one and asserts the
next create past the pool reports `OUT OF THREAD SLOTS`, which is the
documented behaviour for a detached teardown.

Claim: phase-460-W6. Depends on: none. Owns: a new host test under tests/zephyr/ for zephyr/nros_platform_zephyr_shims.c. Gate: the N+2 create/join test in just ci-l1. Status: not started.

### W7 - a fault reaches a hook a console-less board can read (issue 1425)

The hook exists: `nros_platform_panic` (`platform.c:1265`, RFC-0077,
`printk` then `k_panic()` so an image's `k_sys_fatal_error_handler` runs),
and the boot report is the record that survives it. Two faults do not reach
either:

* Heap exhaustion (`platform.c:212`) prints and returns NULL. Decision: it
  writes the boot report's `failed_alloc` fields (they exist for the arena
  case, issue 0900) and then calls `nros_platform_panic` when
  `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL=y` (new, default y on a
  `CONFIG_NROS_BOOT_REPORT=y` image, default n otherwise so a development
  image keeps its NULL-and-log behaviour). A returned NULL is only ever
  handled by code that was written to handle it, and on the island nothing
  was.
* `BufferTooSmall` on the C++ take (`packages/api/nros-cpp/src/subscription.rs:606`)
  returns `NROS_CPP_RET_FULL` with `out_len = 0`; the arena's Rust dispatch
  already counts and logs the drop (`arena.rs:1121`, issue 0757), the C++
  path counts nothing. Decision: the C++ take increments the same per-entity
  drop counter the Rust path uses and the boot report carries the total as
  `samples_dropped_too_small`; the first drop per entity is a `nros_log`
  error line naming the topic and both sizes. Not fatal: a drop is a QoS
  fact, an exhausted heap is not.

Gate: a host test that registers a C++ subscription with a 16-byte buffer,
publishes a 64-byte sample, and asserts the counter is 1 and the log line
names both sizes; a Zephyr native_sim test that exhausts the heap on purpose
with the fatal knob on and asserts the fatal handler ran and the report's
`failed_alloc` names the size.

Claim: phase-460-W7. Depends on: phase-460-W5. Owns: packages/platform/nros-platform-zephyr/src/platform.c (exhaustion path), zephyr/Kconfig (NROS_HEAP_EXHAUSTION_IS_FATAL), packages/api/nros-cpp/src/subscription.rs, packages/core/nros-node/src/boot_report.rs (samples_dropped_too_small), the host test in packages/api/nros-cpp and the native_sim test under tests/zephyr/. Gate: the two W7 tests, the host half in just ci-l1. Status: not started.

## Gates for the phase

| wave | gate | negative control |
| --- | --- | --- |
| W1 | four consumers refuse after a refused resolve | revert, sync, four pass |
| W2 | `refused` fails the configure naming the node | `absent` / `declared` pass |
| W3 | ceiling below bound refused; plain list excludes nested unbounded | ceiling at bound passes |
| W4 | Kconfig 2 vs system 10 fails | equal passes |
| W5 | report reader refuses peak 0 or headroom below 24576 | a dump with headroom passes |
| W6 | N+2 create/join cycles succeed | a detached task exhausts the pool |
| W7 | drop counted and named; exhaustion fatal with the knob on | knob off keeps NULL-and-log |

All but W5's board run and W7's native_sim half run in the fast tier
(`just ci-l1`).

## Limits

* W1 does not make `nros model-path` re-resolve; it refuses. Resolving at
  configure would put the launch parser back on the cmake path that
  phase-296 deleted.
* W3 does not derive `NROS_FRAG_MAX_SIZE` on a workspace with no contract;
  1368's sentinel is the only new form.
* W4 refuses disagreement and changes no precedence. Whether `system.toml`
  should be the only place a domain is written is RFC-0049's question.
* W5 gives the island the instrument; the number it produces is the island's
  to write into its `.conf`.
* W7 names two faults. Issue 1036's "sibling sweep" (every `nros_log` site
  reachable only on a target with no sink) is still the open enumeration.

## Docs to update

* `docs/design/0063-*` (the model as build artifact): the refusal marker and
  the rule that every consumer verifies provenance.
* `docs/design/0077-*`: the two new fault sources and the boot-report fields.
* `docs/design/0049-*`: the domain agreement check, stated as a refusal on
  disagreement rather than a rung.
* `packages/platform/nros-platform-zephyr/src/platform.c:256`: the stale
  `heap-stats` feature name.
* The island's `docs/nxp-deployment.md` "Gaps in the derivation, upstream"
  list (section 10): each row points at its issue here.

## Explicitly not in this phase

* Issue 1121 (a new sidecar is not a freshness input). Its fix is in the
  RESOLVER's input recording, and W1's second hash input makes it visible
  rather than fixing it.
* Issue 1370 (no external-fragmentation bound for the TLSF arena): a bound
  to compute, not a check to add.
* Issue 1352 (`set_parameters` requests larger than the service slot): W2
  removes one way its inputs grow silently and does not size the slot.
* Any change to what the C++ emitter monitors on target (the Rust-only
  `MonitorSpec` table): a phase of its own.
