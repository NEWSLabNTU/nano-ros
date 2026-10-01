---
id: 1555
title: "The `nros-metadata.json` `entities` key has no production producer — the
  reader survives only to build the declared-QoS compile fixture"
status: resolved
resolved_in: 2026-10-01
type: tech-debt
area: [tooling, build]
related: [1265, 1556, 1142, 1407, phase-403, phase-412, phase-454, rfc-0098]
---

## What

`ENTITIES` exists in this tree as **three populations**, and issue 1265 names
only one of them. This issue is the second: the `"entities"` key of
`nros-metadata.json`.

The cmake PRODUCER is gone. `_nros_metadata_emit()`
(`cmake/NanoRosNodeRegister.cmake`) lost the `_entities_field` splice point in
phase-454 W9, and `scripts/check/check-knob-single-reader.py` holds a
`Retired(...)` entry that forbids any cmake file writing the key again. The
Cargo-manifest spelling is separately refused by
`orchestration::nros_config::refuse_retired_entities_key`. A standalone leaf
states its entities in `system.toml` and reaches the pools through
`nros ws entity-facts`, never through this file.

The READER is still there:
`packages/cli/nros-cli-core/src/cmd/entity_inventory.rs`'s
`ComponentMeta::entities: Option<Vec<String>>`, consumed by
`inventory_from_metadata`.

## Measured (2026-09-29)

In a fully-populated checkout — `zephyr-workspace/build-*`, the example leaf
build dirs, the workspace configures:

```
$ for f in $(find . -name nros-metadata.json -not -path './packages/cli/nros-cli-core/tests/*'); do
      grep -q '"entities"' "$f" && echo "HIT: $f"; done; \
  find . -name nros-metadata.json -not -path './packages/cli/nros-cli-core/tests/*' | wc -l
```

| | count |
| --- | --- |
| `nros-metadata.json` paths scanned | **255** |
| carrying an `"entities"` key | 7 |
| of those 7, agent-worktree copies of one committed TEST fixture | **7** |
| **real build artifacts, and of those, hits** | **248 / 0** |

All seven hits are `tests/fixtures/refused_resolve/nros-metadata.json` under
`.claude/worktrees/*` — the same committed test document the top-level
`-not -path` excluded, reached again through the worktrees. It carries the key
decoratively: `refused_resolve_leaves_no_model.rs` asserts nothing about entity
counts. So **0 of 248 build artifacts** carry it.

So on every real road the field is `None`, the declaration is
`Declaration::Absent`, and the metadata inventory contributes **no entities at
all**.

## What keeps it alive

One consumer, and it is a test input:

`packages/api/nros-cpp/tests/compile/declared-qos-fixture/entities.json` is a
committed `nros-metadata.json`-shaped document holding
`sub:std_msgs/msg/Int32:/chatter@depth=1`. `EntityInventory::
to_declared_qos_header` renders `nros/nros_declared_qos_generated.h` from it,
and that header is what `just check c` and `just check cpp` compile their
declared-depth `_Static_assert` TUs against — four `-I` call sites in
`just/check/lanes.just`, a positive TU that must compile and a probe TU that
must NOT. The committed header is held to the emitter by
`the_committed_compile_fixture_is_what_this_emitter_renders`.

Delete the reader and that gate loses its input.

## Why it is not a one-line deletion

`nros ws entity-inventory --output-header` accepts exactly two inputs:
`--metadata` (this key) and `--model` (a resolved SystemModel carrying a
contract sidecar's wiring). The real road is `--model`; in the production
call site (`cmake/NanoRosNodeRegister.cmake`) BOTH are passed and only the
model says anything.

The obvious conversion — commit a small SystemModel beside the fixture and drop
the metadata one — **is banned**. `check-no-tracked-models` (phase-330 W7.e)
refuses a tracked `*_model.yaml` / `*/system_model.yaml` outright, and all 99
resolved models in this tree are `system_model.yaml`, so the gate's reach does
cover it. The model is a build artifact; a committed one re-opens the issue-0380
hand-edit class.

## Fix direction (not decided)

Make the declared-QoS compile fixture's input what a real image's is: resolve a
tiny contract sidecar into a model at FIXTURE BUILD time (an
`examples/fixtures.toml` row, per "no compilation inside tests"), render the
header from `--model`, and then retire `ComponentMeta::entities`, its ~12 unit
tests and `entities.json` together — adding the reader to the existing
`check-knob-single-reader.py` `Retired(...)` entry so it cannot come back.

Cheaper alternative, and probably wrong: teach `entity-inventory` a `--leaf`
input reading `system.toml` `[[component]] entities` and restate the fixture as
a leaf. That retires population 2 onto population 3 — the surface issues 1265
and 1556 exist to retire — so it moves the debt rather than paying it.

## Acceptance

`just check c` and `just check cpp` still fail their probe TU (the negative
control must stay reachable), `ComponentMeta::entities` is gone, and
`check-knob-single-reader.py` refuses its reintroduction on the Rust side as it
already does on the cmake side.

## Corollary already recorded in the code

Because the field is always `None` in production, the documented reason for
`EntityInventory::merged_per_kind_max` — *"the contract has no timer entity, so
a model-only inventory under-sizes MAX_CBS by one per timer"* — describes a
contribution that is **empty today**. The merge is still load-bearing for a
different reason (`self.components` is the REGISTERED component population issue
1407 needs for the node table), but the entity terms come from the model alone.
Noted at the field in `cmd/entity_inventory.rs` so the next reader does not
trust the older comment.

## Which question the compile check answers — and the EXPIRY on this block (2026-09-29)

The verdict above ("do not remove, a fixture needs it") is a block, and a block
with no expiry is the shape this repo already regrets elsewhere — a reason
nobody re-examines. So: **what those two lanes assert, what they do not, and
what would change it.**

Two readings were on the table. The check is either *about the reader* (a
dead-but-tested code path, in which case `check c` / `check cpp` assert nothing
about any shipping image) or *about declared depth reaching a header* (in which
case the fixture sits on a retired input and should migrate to the live one).
**The first is what holds today. The second is not the reason.**

The emitter, the X-macro table, the C11 query form and the
mismatch-fails-the-build property are all SHARED with the live `--model` road,
and the production call site in `cmake/NanoRosNodeRegister.cmake` already passes
`--model` beside `--metadata`. Only the INPUT ADAPTER is dead. So migrating the
fixture to a contract sidecar would not by itself make the lane measure a road
anybody walks, because of this:

**No C++ image in this tree declares a QoS depth.** Of the **7** contract
sidecars under `examples/`, exactly one declares a `qos: depth:` —
`examples/native/rust/listener/system.contract.yaml` — and it is a RUST leaf.
The six C++ contracts (`workspaces/cpp` ×5, `workspaces/derived-tiers-cpp` ×1)
declare none. The `_Static_assert` header emitter is reached from
`nano_ros_node_register`, i.e. the C/C++ road (RFC-0100 D10 already notes the
declared-QoS check is C++-only today). **So the one declared depth in the tree
is in the language with no check, and the images with the check declare
nothing.**

Stated as one sentence: `check c` / `check cpp` demonstrate that the
declared-depth machinery **works**, without demonstrating that any shipping
image **uses** it.

That is not a defect in the machinery, and it is deliberately not filed as one:
`nros/declared_qos.hpp` makes "undeclared ⇒ nothing to disagree with" an
explicit, documented policy (`declared_depth_or`, *"an image that has not opted
in is not in error"*; `NROS_ASSERT_DECLARED_DEPTH` opens with an
`== DECLARED_DEPTH_UNDECLARED ||` short-circuit; and `COUNT` exists separately
from the array's length precisely so *"the table holds nothing"* is a different
claim from *"declared depth zero"*). An opt-in check with a documented opt-out
is a policy; disagreeing with it would be an RFC question, not a bug.

### EXPIRY — when this block lifts

**When one C++ image declares a depth in its contract.** Then the fixture can be
a build-step artifact of that image (`examples/fixtures.toml`, per "no
compilation inside tests"), the header comes from `--model` as it does in
production, `ComponentMeta::entities` retires for free, and the lane starts
asserting something about a shipping binary. Until then the metadata reader
stays, and this issue is the record of why — not a permanent exemption.

### Measurement caution for whoever re-checks these numbers

The fixture-vs-production half of this issue is **build state, not a property of
the tree**, and it cannot be re-derived from a clean clone. Measured on THIS
checkout: there is **no generated production declared-QoS header at all** —
`find . -name 'nros_declared_qos_generated.h' -not -path './.claude/worktrees/*'`
returns only the committed fixture's. An earlier draft of this issue reported
"2 production headers, both refused"; both were in fact inside
`.claude/worktrees/agent-*/`, i.e. another agent's build output in a shared
checkout, reported as this tree's.

The same tool caused a second wrong number here, in the other direction: a
`find` for `*.contract.yaml` returns 44, because it walks through the gitlink
into `packages/cli/third-party/play_launch` — a different repository, whose 27
contract fixtures say nothing about nano-ros. **nano-ros tracks 17**, of which
**10** are CLI test fixtures under `packages/cli/nros-cli-core/tests/fixtures/`
and **7** are the example contracts this issue reasons about. Re-derive the gap
with:

```
comm -13 <(git ls-files '*.contract.yaml' | sed 's|^|./|' | sort) \
         <(find . -name '*.contract.yaml' -not -path './.claude/*' | sort)
```

Every line comes back under `play_launch`. The practice, for a claim about THIS
tree: count with `git ls-files`, and treat a `find` count as answering a
different question — it sees other agents' worktrees and other repositories.
(This is the correctness sibling of `scripts/check-no-tracked-file-find.sh`,
whose rule is the same and whose stated rationale is performance. Nothing here
asks for that gate's allowlist to be widened: a `.contract.yaml` can legitimately
be untracked build output, since `nros sync` synthesises one into a generated
dir.)

## Resolution (2026-10-01)

**The reader is retired, and the fixture moved to the road a real image takes.**
Fix direction 1 -- "render the header from `--model`" -- without the build-step
fixture the issue assumed it needed, because the premise behind that assumption
was wrong: a model CAN be committed as a test input. `check-no-tracked-models`
refuses `*_model.yaml` / `*/system_model.yaml`, and the tree already held a
precedent beside this very fixture: `declared-params-fixture/declared_params.yaml`
(phase-446 W6) is a SystemModel, named so the gate does not mistake a test input
for a build artifact. The declared-QoS fixture now has the same shape.

What changed:

* **`packages/api/nros-cpp/tests/compile/declared-qos-fixture/declared_qos.yaml`**
  (new) -- the model. It states the same three cases the metadata document did:
  `/chatter` at `qos: { depth: 1 }`, `/undeclared` with no depth, and a timer
  path (`contracts.node_paths./listener/on_tick`). The issue's corollary said "the
  contract has no timer entity"; it does -- a `node_paths` row with no `input` is
  one, and `EntityInventory::from_model` already counted it.
* **`entities.json` -> `nros-metadata.json`**, with the `"entities"` key removed:
  it now says what a configure's metadata says -- which components are
  registered -- and nothing about what they create.
* **The header** was regenerated through the CLI verb a configure runs
  (`nros ws entity-inventory --metadata ... --model ... --component demo::listener
  --output-header ...`). Only its `Source:` comment line changed: the X-macro rows,
  `NROS_DECLARED_QOS_STATUS` and `NROS_DECLARED_QOS_UNDECLARED_COUNT 1` are
  byte-identical, so `just check c` / `just check cpp` compile the same table and
  the probe TUs fail for the same reason. `MAX_CBS` from the verb is 3 on both
  inputs (two subscriptions and the timer).
* **`ComponentMeta::entities`** is now `Option<serde::de::IgnoredAny>` and a
  document carrying the key is REFUSED, naming this issue and `--model`. Not
  dropped from the struct: serde ignores an unknown key, so a document still
  carrying one would have had its declaration discarded in silence.
* **`fold_model`** -- the model fold `run` performs, lifted out so
  `the_committed_compile_fixture_is_what_this_emitter_renders` holds the header to
  THAT function (plus the three refusals `run` applies at the same door) rather
  than to a restatement of it.
* **`check-knob-single-reader.py`** gained a `Retired` row scoped to
  `struct ComponentMeta`, forbidding an `entities` field that could hold the
  strings. Negative control run: changing the field back to
  `Option<Vec<String>>` fails the gate naming this row; restoring it passes.
* The reader's own tests went with it (`entities_travel_...`,
  `an_empty_entities_list_...`, `none_beside_entities_...`,
  `a_bad_spelling_...` -- the grammar they exercised is `EntityDecl::parse`,
  still tested where `system.toml` reads it). Replaced by
  `a_metadata_row_alone_is_absent_never_zero`,
  `the_retired_entities_key_is_refused_not_ignored` and
  `the_model_states_what_the_registered_components_create`; the narrowing and
  ambiguity tests now compose a model instead of declaring in metadata.
* `tests/fixtures/refused_resolve/nros-metadata.json` carried the key
  decoratively (the issue noted its test asserts nothing about entity counts);
  removed, since the verb now refuses a document that carries it.

Acceptance, checked: the probe TUs still fail (the header's rows are unchanged
byte for byte), `ComponentMeta::entities` no longer reads anything, and the gate
refuses its reintroduction on the Rust side as it already did on the cmake side.

**What this does NOT change**, and is not claimed: the "EXPIRY" section above
still holds. The lanes now compile against a header rendered from the live
input adapter, but no C++ image in the tree declares a depth in its contract, so
`check c` / `check cpp` still demonstrate that the machinery works rather than
that a shipping image uses it. That is the documented opt-in policy, not a
defect, and it lifts on the condition stated there.
