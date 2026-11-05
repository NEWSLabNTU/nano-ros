---
rfc: 0122
title: "Example pairs live as images in the language workspaces, one single-node workspace per platform and language stays as the copy-out example, and every support cadence names the CI job that keeps it"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-535]
amends: [0026, 0051, 0061, 0066, 0114, 0121]
supersedes: []
superseded-by: null
---

# RFC-0122 — Behaviour bringups, single-node witnesses, and cadence-backed CI

**Status:** Draft (2026-11-04). Written from the maintainer discussion of
2026-11-04 on test organisation, after the decision to turn standalone apps
into workspaces (RFC-0121). Implemented by
[phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md).

## Summary

RFC-0121 makes every project a workspace and migrates each standalone example
**in place**: `examples/<plat>/<lang>/<example>/` gains a `src/` and a
bringup. That keeps 275 example fixture rows as 275 separate workspaces, each
with its own `nros configure`, its own CMake configure and, on the CMake RTOS roads, its
own build of the kernel, network stack, RMW and `nros-c`.

This RFC moves the example **pairs** (talker/listener, service
server/client, action server/client) somewhere else. They become `[image.*]`
rows in the language workspaces `examples/workspaces/{rust,c,cpp}`, under one
bringup per behaviour (`pubsub_bringup`, `service_bringup`, `action_bringup`).
One single-node workspace per (platform, language) stays at today's talker
path. It is the copy-out example a reader starts from, and the witness for
what the merged images no longer show: a one-component image, a generated
launch file, and the platform's build driver run from its own directory.

The tests do not change. A cell's `Kind` says what the test does, not where
its image is built, so every test file and rstest case name stays. Only the
fixture resolver learns to find an image in a workspace. A coverage ratchet
derived from the manifest refuses any change that drops the last witness of
a (platform, language, driver, project shape, launch origin, entry form). Every
example uses the generated entry; a hand-written `main` stays only where an
example exists to demonstrate one (D10).

The CI half binds each support cadence of RFC-0114 to a named workflow job,
because today two of them have none. `cadence = "merge"` (linux, zephyr
native_sim, threadx-linux) is backed by nothing that runs at merge, and sched task
3 by no workflow at all. A cadence the CI cannot afford is lowered in the
registry, not left unbacked.

## Problem

### Measured 2026-11-04 against `examples/fixtures.toml`

275 `[[fixture]]` rows build a directory under `examples/` that is not a
workspace or a template:

| kind | rows | what it is |
| --- | --- | --- |
| pair role (the six roles) | 225 | one leaf per (platform, lang, role); its RMW is a row field |
| variant of a role (`-rtic`, `-rtic-mixed`, `-async`, `-callback`) | 22 | different node code for the same role |
| feature demo (logging, parameters, custom-msg, lifecycle, safety, typed-carrier, errno) | 13 | a feature node |
| configuration (`serial-*`, `custom-transport-*`) | 7 | the same node code with a different link or knob |
| rmw variant (`talker-xrce`) | 1 | the same node, another RMW |
| `custom-platform` | 1 | a user-supplied platform port |
| templates | 6 | copy-out scaffolds |

Every RTOS platform carries the same six role directories per language.
`examples/workspaces/rust/src/` already holds `talker_pkg`, `listener_pkg`,
`service_{server,client}_pkg` and `action_{server,client}_pkg`. The roles
match, but **the programs do not** (amended 2026-11-04, option B below): the
examples are the ROS 2 tutorial programs (`std_msgs/String` "Hello World: N",
a one-shot `add_two_ints` client, an action client that streams feedback), and
the workspace packages are an Int32 demo (`sent: N`, a client calling every
tick, a result-only action client). So the build around the node code is
duplicated, and the node code itself is two programs per role.

The precedent is RFC-0066's own measurement (2026-08-03). Folding 35 sync and
configure cycles into 15 made the native stage **21.5 % faster per fixture**
while it built 8 more fixtures.

### Two promises have no CI behind them

From the workflow survey of 2026-11-04 (origin/main `68573185a6`):

- `book/src/reference/support-status.md` promises tier 1 on "every change".
  `packages/boards/board-support.toml` gives linux, zephyr native_sim and
  threadx-linux `cadence = "merge"`, which RFC-0114 defines as "a
  merge-gating lane". But the merge gate builds no fixture: `gate.yml` is
  compile and unit tests only (CLAUDE.md, `ci gate`). The runtime evidence
  for those three boards comes from `run-matrix.yml` on a daily cron, on one
  self-hosted runner.
- Tier 3 (`ci full`, "pre-release") is called by no workflow, including
  `release-nros.yml`.

Also: the Zephyr nightly builds 20 hard-coded leaves, zenoh only, without
running them (`nightly.yml` 815-829, 936-939). Yet the registry lists
mps2-an385 Zephyr as verified in the emulator daily.

## Design

### D1 — The pairs are images in the language workspaces

For each behaviour, each language workspace gains one bringup:

```
examples/workspaces/<lang>/src/
├── talker_pkg/ listener_pkg/ service_*_pkg/ action_*_pkg/   (exist today)
├── pubsub_bringup/    launch/{talker,listener}.launch.xml  system.toml
├── service_bringup/   launch/service_{server,client}.launch.xml  system.toml
└── action_bringup/    launch/action_{server,client}.launch.xml  system.toml
```

A bringup's `system.toml` holds one `[image.<platform>_<role>[_<rmw>]]` row
per (platform, role, rmw). The id follows `README-layout.md`'s one platform
vocabulary, and zenoh takes no suffix. One process per role stays. An image
that hosts every role and picks one at run time is **rejected**: it would be
sized for every node, and the per-image sizing class issue 1378 found would
stop being visible in the pair cells.

Splitting by behaviour rather than one bringup keeps each `system.toml`
readable: `demo_bringup` already has 17 rows, and every pair in one file would
pass 100. It also keeps any shared-runtime descriptor fold (RFC-0100 D12)
inside one behaviour. `demo_bringup`'s existing `linux_service_*` and
`linux_action_*` images move into the behaviour bringups, so each image exists
once.

**One program per role (option B, maintainer ruling 2026-11-04).** Before any
pair moves, the workspace node packages of `rust`, `c` and `cpp` are rewritten
as the tutorial programs the examples run, and every workspace test, contract
(`system.contract.yaml`), sizing measurement and marker expectation that
reads the old Int32 demo moves with them. The examples' shape wins because it
is what a ROS 2 user expects and what `ros2 topic echo` and the interop cells
already read. A component may not block its executor, so a client sends its
one request from a timer and polls for the reply; it prints the same line the
blocking tutorial client prints. The `mixed` workspace keeps its own packages:
it shares none with the three language workspaces, and its purpose is the
cross-language composition, not the tutorial.

### D2 — What each non-pair row becomes

The rule is RFC-0066's, applied one level down:

| row kind | becomes |
| --- | --- |
| variant (different code) | its own node package (`service_client_callback_pkg`) plus an image row in the behaviour's bringup |
| feature demo | a node package (in `workspaces/features` on linux, the language workspace on an RTOS that runs it) plus an image row |
| configuration (`serial-*`, `custom-transport-*`) | an image row stating the knob (`transport = "serial"`), the same talker/listener package |
| rmw variant | an image row with `rmw = "…"` |
| `custom-platform` | stays a single-node workspace: it witnesses the user-supplied-platform road |
| templates | unchanged |

### D3 — One single-node workspace per (platform, language) stays

`examples/<plat>/<lang>/talker/` stays, in RFC-0121's shape: one node package
plus a bringup with no `launch/`, so the launch file is generated (RFC-0121
D3). There is one for each (platform, language) that has an example today, 22
in all. The path is unchanged on purpose: `just/ci.just`'s link check,
`rust-rtos-link-check`, `board-support.toml`'s `link_check_example` and the
per-platform recipes already name `rust/talker` and keep working.

What only these show, and the merged images cannot:

- a single-component image, sized for one node;
- a bringup whose launch file is generated, not authored;
- the platform's build driver run from the example's own directory, the way a
  reader who copied it runs it;
- the book's first chapter, which points at them (RFC-0116 D6).

Each one runs, not only builds. A new `road_witness_e2e` pairs each witness
talker with the workspace listener of the same (platform, language, rmw).

### D10 — Examples use generated entries; a hand-written `main` survives only to demonstrate one

Maintainer ruling 2026-11-04. Phase-535 W0 found that the C and C++
standalone examples carry a hand-written `main` running the whole user
lifecycle:
- C: `rclc_support_init`, `rclc_node_init_default`, `rclc_executor_init` /
  `spin_period` / `cancel` / `fini`, `rclc_support_fini`;
- C++: `rclcpp::init`, `create_node`, `spin_once`.

The workspace components call none of it, because the generated entry sets
up the node. Rust examples already use `nros::main!` and are unaffected.

The ruling: every example uses the generated entry, and the book teaches it.
A hand-written `main` stays only in an example whose PURPOSE is to show a
manual entry: porting existing rclcpp code, a user-supplied platform, a
caller-supplied link. Phase-535 W1 lists that set, each with its reason, and
the D5 ratchet carries entry form (generated or manual) as an axis, so the
last manual-entry demo of a (platform family, language) cannot vanish
unnoticed.

The cost, stated rather than smoothed over: the rclc/rclcpp lifecycle runs on
an RTOS only where a manual-entry demo runs on it. Everywhere else it is
covered by host tests. This is RFC-0121 D8's direction ("any other `main` is
reported for a person to move"), made a decision for the examples.

### D4 — A cell's `Kind` is the test's, not the build's

Pair cells stay `Kind::Example`. The fixture resolver maps (cell, role) to
(workspace, bringup, image id), so `zephyr`, `rtos_e2e`,
`linux_example_{pubsub,reqresp}_e2e`, `emulator`, `linux_api`,
`threadx_riscv64_qemu` and `c_riscv_nuttx_e2e` keep their files and their
case names. For example, `example_e2e::case_19_xrce_rust_pubsub_e2e` resolves
`rust/pubsub_bringup/zephyr_listener_xrce` and `…/zephyr_talker_xrce`.

Flipping the cells to `Kind::Workspace` instead is **rejected**.
`matrix::consumer_of` has `(_, _, _, Service | Action, Kind::Workspace) =>
Roundtrip` ahead of the per-platform rules, so every service and action cell
on every platform would leave its runner for `roundtrip_xprocess_e2e`, with no
test failing.

### D5 — A coverage ratchet guards the deletion

A gate on the fast line derives, from the manifest rows, the set of
(platform family, language, build driver, project shape, launch origin)
exercised by at least one runtime cell:

- project shape: single-node or multi-image bringup;
- launch origin: generated or authored;
- entry form: generated, or a manual `main` (D10).

The set may only grow. Its floor is the tree at adoption. Deleting a
directory that was the last witness of a tuple goes red, naming the tuple.
It is derived from rows, never listed by hand, for the reason
`check-default-gates-run-somewhere` gives: an authored list drifts toward OK.

### D6 — Grouping is the saving, and it is measured

The fixture builder groups rows that share (workspace, bringup, board, rmw)
into one configure. A CMake RTOS group builds its runtime once and links each
entry against it, and codegen and `nros configure` run once per workspace.

Rust gains only if entries also share a cargo root. Today each generated entry
is its own root, and a root's units are never shared (issue 0616). Grouping
roots per (board, rmw) is allowed only behind a gate showing that each entry's
resolved feature set equals what it resolves alone. Otherwise cargo's feature
unification hides "this image does not build by itself", the class that hid 20
errors in `nros-node`'s own test target.

Zephyr gains little: every image is its own west build, kernel included, and a
pair needs two images. No saving is claimed before phase-535 measures it with
`scripts/dev/measure-fixture-build.sh`, the method RFC-0066 used.

### D7 — The isolation allocator reaches workspace images

Since issue 2251, the west lane reads every baked router port, Agent port and
domain from `nros_tests::alloc` (`cell-isolation`), keyed by `row_coord` plus
the workload a west *role* is an image of. A workspace image row needs the
same mapping from its image id to a workload. Without it the emit refuses,
which is loud.

### D8 — Every cadence names the job that keeps it

RFC-0114's `cadence` gains a binding: each value names exactly one workflow
job, and that job runs the board's cells at that cadence.

| cadence / tier | job | runner | runs |
| --- | --- | --- | --- |
| `merge` | **`merge-runtime`** (new), in the `CI` aggregator on `merge_group`; one shard per merge board | hosted (zephyr shard in the zephyr-ci image) | the three behaviour bringups in rust/c/cpp, zenoh, plus that board's witnesses, then their pair cells |
| tier 1 (full) | daily, hosted shards, off the self-hosted runner | hosted | `lane=tier1`, every rmw and feature image for the merge boards |
| tier 2, 2-nightly | as today | self-hosted | `ci matrix`, `ci matrix-nightly` |
| `daily` | nightly `platform` jobs; the Zephyr nightly becomes 2 lines × 3 languages = 6 jobs **with native_sim runtime** | hosted | workspace rows of the platform, then `just test <plat>` |
| `weekly` | `probe-platforms`, chapters retargeted to the witnesses | hosted | the book replay |
| tier 3 | **`release-gate`** (new): `ci full` on dispatch; `release-nros` requires a green run on the tagged commit | self-hosted | the whole matrix |

The PR lane stays a compile smoke. It gains only the D5 ratchet and an
image-row check (every `[image.*]` resolves to a board, a launch file and a
cell), both static.

### D9 — A cadence the CI cannot afford is lowered, not kept

`merge-runtime` runs **advisory** on `merge_group` first. If its p95 stays at
or under 20 minutes per shard, it joins `CI`. If not, the three boards move to
`cadence = "daily"` in `board-support.toml`, and `support-status.md` stops
saying "every change". A smaller promise that is kept beats a larger one that
has no job behind it.

## Constraints this rests on

- **Multi-bringup Rust workspaces.** `nros configure` today generates no selection
  facades for a workspace with more than one bringup
  (`packages/cli/nros-cli-core/src/cmd/ws.rs`, `generate_facade_crates`).
  RFC-0121 retires the selection crate (phase-531 W7), which should remove the
  limit. Phase-535 verifies that before D1 lands for Rust, and falls back to
  image rows in `demo_bringup` if it does not.
- **C/C++ capability sets across bringups.** RFC-0066 recorded that with
  several bringups, one bringup's empty capability set erased another's for
  the whole configure (last write wins). The three behaviour bringups declare
  identical capability sets, so the union is safe, but the defect is checked
  rather than assumed.
- **Order.** Phase-531 W2 to W5 (the rule, launch generation, one build road)
  land first. Phase-535 then replaces phase-531 W6 for the pairs under
  `examples/`. W6 still migrates the witnesses, `packages/testing/` and the
  templates in place.

## Amendments to other RFCs

| RFC | Amendment |
| --- | --- |
| 0121 D8 | Under `examples/`, only the single-node witnesses (D3) and `custom-platform` migrate in place. The pairs, variants, configurations and feature demos move into the language and features workspaces (D1, D2) |
| 0066 | "Standalone examples: already conformant" is superseded. A role is an image row, as a configuration already was |
| 0026 | `examples/<plat>/<lang>/<example>/` holds only the single-node copy-out workspaces |
| 0051 | `Kind` names the test, not the artifact's build shape (D4). The fixture resolver, not the cell, knows the workspace |
| 0061 | The tier ladder gains its cadence binding (D8); the tier 1 "every change" wording depends on D9 |
| 0114 | `cadence` names a job (D8), and a value with no job is lowered (D9) |

## Alternatives considered

- **Migrate every leaf in place (RFC-0121 D8 unchanged).** No test or
  directory churn, but it keeps 275 configures and saves nothing.
- **One bringup for everything.** Works under today's multi-bringup limit, but
  a `system.toml` of 100+ rows, and one shared descriptor fold across all
  behaviours. It is the fallback, not the design.
- **One image per platform that runs any role, chosen at run time.** It halves
  the image count on host-simulated boards and gives up per-image sizing (D1).
- **Keep standalone leaves as road witnesses.** That was the first sketch.
  RFC-0121 retires the standalone road, so a witness of it would guard code
  being deleted.

## Open questions

- Whether the fixture builder already groups `[[workspace_fixture]]` rows that
  share (dir, bringup, board, rmw) into one configure. Phase-535 W2 answers it
  before anything moves.
- Whether Jazzy and Iron wire interop, listed "Supported" in
  `support-status.md`, needs a workflow. Every CI container is Humble, so none
  backs it today. Out of scope here, recorded so it is not lost.
- One self-hosted runner carries tier 2, 2-nightly and the queue's cross-build
  check. Moving tier 1 to hosted shards (D8) relieves it, but it stays a
  single point of failure.

## Changelog

- 2026-11-04: Draft, from the maintainer discussion of 2026-11-04 (behaviour
  bringups, single-node witnesses, cell `Kind` kept, cadence-backed CI).
- 2026-11-04: D10 (maintainer ruling): examples use generated entries, and the
  book follows. A hand-written `main` stays only in a manual-entry demo. The
  ratchet gains the entry-form axis.
- 2026-11-04: Option B (maintainer ruling): the node code was not already
  shared. D1 now unifies the workspace node packages on the tutorial programs
  before the pairs move; Problem amended to say so.
