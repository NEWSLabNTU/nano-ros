# Phase 535 — Example pairs into the language workspaces, single-node witnesses, and cadence-backed CI

**Status (2026-11-05). OPEN — W0 answered (issue 2253 fixed on the way,
#3106); W1 merged (#3107); W0.b (one program per role): C merged (#3112),
C++ merged (#3120), Rust in review; W2 next.**
Implements [RFC-0122](../design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md)
D1 to D9, with its amendments to RFC-0026, RFC-0051, RFC-0061, RFC-0066,
RFC-0114 and RFC-0121 D8.

Depends on:

- [phase-531](phase-531-workspace-first-project-layout.md) W2 to W5: the
  bringup rule, launch generation and the one build road. This phase replaces
  phase-531 W6 for the pairs under `examples/`, and W6 still migrates the
  witnesses, `packages/testing/` and the templates in place;
- phase-531 W7 step 3 (the selection crate retires) for Rust workspaces with
  more than one bringup. W0 below checks this rather than assuming it;
- issue 2251 (merged): the west lane reads isolation values from
  `nros_tests::alloc`.

## Why

275 example fixture rows build 275 separate projects whose node code the
language workspaces already hold. On the CMake RTOS roads, each one builds its
own kernel, network stack, RMW and `nros-c`. Separately, two support promises
have no CI job: `cadence = "merge"` for linux, zephyr native_sim and
threadx-linux, and tier 3. RFC-0122 has the measurements and the workflow
survey.

## Work items

### W0 — Can a workspace hold several bringups?

- Build a Rust workspace with two bringups on the phase-531 tree and check
  that each bringup's images build with their own selection. Today
  `generate_facade_crates` (`packages/cli/nros-cli-core/src/cmd/ws.rs`)
  declines to generate facades for it.
- Reproduce RFC-0066's C/C++ open question: with three bringups, does one
  bringup's capability set still overwrite another's for the whole configure?

**Acceptance:** both answered with a build. If either fails and is not fixed
here, D1 uses the fallback (image rows in `demo_bringup`) for that language,
and RFC-0122 records it.

**Result (2026-11-04), both answered with a build, neither needs the
fallback:**
- A Rust workspace with a second bringup builds. Each image gets its own
  selection crate (`generated/nros-selection/<entry>`) with the right RMW
  (posix-xrce for an xrce image), and `demo_bringup` still builds beside it.
  `nros configure`'s "selection facades not generated for multi-system
  workspaces" warning is harmless for generated entries.
- The C/C++ capability last-write-wins is unreachable here: no bringup
  declares capabilities.
- Grouping already exists for CMake: images of one (board, rmw, config hash)
  share a configure (`build/posix-zenoh-linux/cmake` holds seven C images).
  Each Rust entry has its own cargo target; nothing is shared beyond sccache.
- The builder qualifies an image as `<bringup>:<image>`
  (`scripts/build/workspace-fixtures-build.sh`); tests resolve it with
  `build_workspace_cmake_entry(row_id, lang, "<image>_entry")` and its cargo
  twin.
- On the way, issue 2253: a generated entry kept the board's panic crate
  under `panic = "platform"` (E0152). Fixed by `[board.entry]
  own_panic_root_*` and `BoardFacts::for_image_panic`.

### W0.b — One program per role (option B)

Maintainer ruling 2026-11-04, recorded in RFC-0122 D1: the example pairs and
the workspace node packages were different programs (tutorial `String`
versus an Int32 demo). The workspace packages of `c`, `rust` and `cpp` become
the tutorial programs, and every test, contract, sizing measurement and
marker expectation that read the Int32 demo moves with them, through
`nros_tests::output` constants. Order: C (one gate run), then Rust and C++,
then the pairs move (W3).

Decisions taken here, with the reason:
- **One-shot client.** The tutorial client sends one request (2 + 3) and
  prints `Result of add_two_ints: 5`. `roundtrip_xprocess_e2e` used to want
  three sums from a client that called every tick; it now asserts the one
  tutorial result, plus, for actions, the whole order-10 sequence and at
  least one feedback line. Repeated calls were a property of the demo
  client, not of the cross-process path the cell exists to prove.
- **A component never blocks.** The client sends from a timer and polls for
  the reply, with a resend guard for a request lost before discovery, and
  prints what the blocking tutorial client prints.
- **Observers follow the type.** While the C++ and mixed talkers still
  publish Int32, `entry_e2e` observes them with the `int32-sink`, and that
  row `serves` their coordinates (a second serves-equality test keeps it
  honest); the C robot2 listener observes the C cells.
- **`mixed` keeps its packages.** It shares none with `c`, `rust` or `cpp`,
  and its purpose is cross-language composition.
- **One bound per stock string type (C++).** The C++ workspace stated
  `std_msgs/String` at 16384 for phase-441 W6's fragmented leg
  (`bulk_listener_pkg`). With the tutorial pair on that type, the listener's
  receive hint (16400 B) exceeded the largest zenoh payload class and the
  subscription declare failed (`rmw_ret error`), and every image would have
  carried a 16 KiB message. The bulk listener moved to
  `example_interfaces/String` (16384) and `std_msgs/String` is 256, as in the
  C workspace and the standalone examples. Measured cost of the tutorial pair
  over the Int32 demo: `linux_entry` .bss +256 B, `threadx_entry` .bss +512 B.
- **The `int32-sink` observer stays.** With C, C++ and Rust on String, its
  `/chatter` role serves only the mixed cells, whose talker publishes Int32.
  It is also the side-topic sink of the param, safety, QoS, realtime, bridge
  and sizing tests (`/ctrl`, `/telem`, `/safe_ok`, …), none of which
  phase-535 touches, so retiring it is not on the table.
- **Rust nodes are the examples' nodes.** The Rust examples' node logic is
  already one file per role, byte-identical across platforms, so the
  workspace packages take that logic verbatim and keep their own class and
  node names (which the bringup, launch files and tests address). The
  in-process service test now reads the client's own result line; it used to
  need an `int32-sink` on `/sum`.

**C++ (2026-11-05):** done. `entry_e2e`'s C++ RTOS cells observe through
the C robot2 listener again and `serves` moved back; roundtrip, multihost,
freertos-posix, the an536 heap and both an536 Cyclone ROS 2 interop tests
read the tutorial markers. Run green on rebuilt fixtures: linux,
threadx-linux, freertos (mps2 zenoh, an536 Cyclone + slirp), freertos-posix
and the two zephyr native_sim west leaves.

**Rust (2026-11-05):** in review. Run green on rebuilt fixtures: linux
(roundtrip, in-process service, deployed system, multihost, per-node graph on
three RMWs, features, metadata) and zephyr native_sim (workspace entry,
multihost robot1). The FreeRTOS bridge pair (`bridge_network_e2e`) needs the
`qemu-br` host bridge and was not run here.

**Acceptance:** each language's workspace tests are green on the lanes that
build its images; no Int32 demo marker is left in a language workspace.

### W1 — The coverage ratchet and the image-row check

- The D5 gate: derive (platform family, language, build driver, project
  shape, launch origin) per runtime cell from the manifest. Fail when a tuple
  in the floor has no cell. The floor is a committed snapshot of the tree at
  adoption, which may only grow.
- The image-row check: every `[image.*]` in a bringup resolves to a known
  board, an existing or generated launch file, and a matrix cell.
- Both are static, on the fast line.

- The manual-entry inventory (RFC-0122 D10): every example `main` that is not
  `nros::main!`, classified as a **manual-entry demo** (kept, with its reason
  written down) or a **generated-entry candidate** (moves to a component).
  Candidates so far: the port templates (`cpp-port-minimal-publisher`,
  `rclcpp-compat-smoke`), `custom-platform`, and `custom-transport-*`. The
  ratchet carries entry form as an axis.

**Acceptance:**
- A mutation that deletes the last witness of a tuple fails the ratchet,
  naming the tuple.
- A mutation that adds an image with an unknown board fails the row check.
- Both mutations are recorded in the commit.

**Done (#3107).** `scripts/check-example-coverage-ratchet.py` (floor
`.config/example-coverage-floor.txt`, 75 tuples at adoption) and
`scripts/check-image-launch-files.py` (262 `system.toml`), both on the fast
line. Board resolution was already `check-deploy-board-resolves`, so the row
check covers the launch file.

### W2 — Grouping, the resolver, and the allocator lookup

- Find out whether the fixture builder groups `[[workspace_fixture]]` rows
  that share (dir, bringup, board, rmw) into one configure. If not, make it.
- `nros_tests::fixtures::workspace_image`: (cell, role) → (workspace,
  bringup, image id). This is the only place that knows the mapping (D4).
- `zephyr-fixture-leaves.sh`: workspace west rows map their image id to a
  workload for the `cell-isolation` lookup (D7).

**Acceptance:**
- One grouped configure builds two images of one bringup, and `ninja -t
  query` shows one runtime library feeding both.
- The resolver is unit-tested on every pair cell in `matrix::CELLS`.

### W3 — Pilot: `mps2-an385-freertos`

- Move its 18 pair rows into `{rust,c,cpp}/src/{pubsub,service,action}_bringup`
  as `freertos_*` images. Keep `examples/mps2-an385-freertos/<lang>/talker/` as
  the witnesses.
- Measure with `scripts/dev/measure-fixture-build.sh`, cold, before and after,
  as RFC-0066 did. Record it in `docs/roadmap/data/phase-535-w3-pilot.md`.

**Acceptance:**
- `just test freertos` is green, with the same rstest case names as before.
- The ratchet is green.
- The measured saving is recorded, whatever it is.
- A saving under 10 % stops the phase for a maintainer decision before W4.

### W4 — The merge boards: linux, zephyr native_sim, threadx-linux

Same move as W3, all three RMWs. `demo_bringup`'s `linux_service_*` and
`linux_action_*` images move into the behaviour bringups, so each image exists
once.

**Acceptance:** `just ci tier1` is green, and
`zephyr::example_e2e` keeps its 27 case names. The cross-Agent XRCE repro of
issue 2221 (two Agents, two cells) hears 0 foreign samples.

### W5 — The other platforms

nuttx (arm, riscv), threadx-riscv64 (with its cyclonedds rows) and
mps2-an385 bare metal (with the `-rtic` and `-rtic-mixed` variants per D2).
One PR per platform directory, each rebuilding its own lane.

**Acceptance:** `just test <platform>` is green for each; `just ci matrix`
(tier 2) is green at the end.

### W6 — Variants, configurations and feature demos

- The 22 variants become node packages.
- The 7 configurations become image rows with knobs.
- The 13 feature demos become node packages in `features` or the language
  workspace.
- `talker-xrce` becomes an `rmw` row.

All per RFC-0122 D2.

**Acceptance:** each row's consumer test is green; the ratchet floor is unchanged.

### W7 — Retire the old leaves, add `road_witness_e2e`

- Delete every pair, variant, configuration and feature leaf moved by W3 to
  W6. Keep the 22 witnesses and `custom-platform`.
- `road_witness_e2e`: each witness talker against the workspace listener of the
  same (platform, language, rmw).
- Sweep every hard-coded leaf path from the 2026-11-04 survey:
  - `nightly.yml`: path filters 197-203, the 10 Zephyr leaves 819-829, `c/talker`
    at 1036 and 1142;
  - `just/ci.just` 229-231, and `justfile` `rust-rtos-link-check`;
  - `board-support.toml` `link_check_example`;
  - `just/zephyr-dev.just`, `just/zephyr-ci.just` 238;
  - the per-platform `just/*.just` recipes;
  - `justfile:360`.

  Most of these name `rust/talker` and keep working. Each one is checked, not
  assumed.

**Acceptance:**
- `git ls-files 'examples/*/*/*/'` lists only witnesses, `custom-platform`
  and templates.
- `just ci full` is green with a lower fixture-configure count.
- The book's example pointers resolve.

### W8 — CI follows the cadences

1. `merge-runtime` on `merge_group`, **advisory**: one hosted shard per merge
   board, running the behaviour bringups (zenoh, three languages) plus that
   board's witnesses, then their pair cells.
2. After a week, if p95 is at most 20 min per shard, it joins `CI`. Otherwise
   the three boards move to `cadence = "daily"` and `support-status.md`
   stops saying "every change" (RFC-0122 D9).
3. Tier 1 full moves to hosted shards, daily.
4. The Zephyr nightly goes from 20 hard-coded build jobs to 6 (2 lines × 3
   languages), with native_sim runtime.
5. `release-gate`: `ci full` on dispatch; `release-nros.yml` requires a green
   run on the tagged commit.
6. Stale headers, fixed against the workflows they describe:
   - `nightly.yml:25` (no PR trigger)
   - `post-submit.yml:38-40`
   - `queue.yml:95-99`
   - `docs/development/multi-agent-ci-workflow.md` 36-44
   - `support-status.md` tier-2 trigger wording

**Acceptance:**
- Every `cadence` in `board-support.toml` names a job that ran in the last
  window. A gate reads both files and refuses a cadence with no job.
- A release cannot be cut without a green tier 3 on its commit.

### W9 — Docs

- RFC-0122 flips to Stable.
- The book teaches the generated entry (RFC-0122 D10). Coordinated with
  phase-531 W8, which rewrites the same chapters for the workspace layout; a
  hand-written `main` appears only in the manual-entry chapter.
- `examples/README.md`, `examples/workspaces/README-layout.md` (the new
  bringups and the image-id vocabulary), the book's first chapters (witness
  paths), and `book/src/reference/support-status.md`.
- One pointer line in CLAUDE.md.

## Acceptance (phase)

- The ratchet floor from W1 holds at every merge of this phase.
- Rstest case names in the pair consumers are unchanged.
- The measured fixture-build time of `lane=all` is recorded before W3 and
  after W7.
- Every RFC-0114 cadence has a job (W8), or has been lowered.
