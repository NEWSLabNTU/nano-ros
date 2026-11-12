# Phase-555 W2, slice `workspace-system-model`: verdicts and decision brief, 2026-11-13

The workspace-system-model slice of [phase-555](../phase-555-design-doc-review.md)
W2. The area has 21 RFCs and two reviewers. **This file covers reviewer
142's half: RFC-0004, 0015, 0024, 0025, 0026, 0045, 0046, 0047, 0050, 0052
and 0060.** Reviewer 133's half (0063, 0074, 0078, 0079, 0086, 0100, 0103,
0121, 0122, 0125) is merged in below from #3673.

Every fact is cited to the file and line it was read from on 2026-11-13, at
`8bdca83cb4` (branch `docs/555-w2-review-wsm`, off `origin/main`).
Mechanical fixes were made in the RFCs themselves, in the same commit as this
file. Nothing here changes a design until it is ruled.

Inputs: `build/rfc-inventory/slices/workspace-system-model.md`. The pairs in
this half are three tier-1 pairs (0004-0031, 0024-0025, 0050-0052) and seven
tier-2 pairs (0004-0016, 0015-0032, 0017-0047, 0022-0052, 0024-0065,
0060-0091, 0060-0111). The pairs whose newer side is 133's (0063, 0074, 0100,
0103, 0109, 0121) were read for their effect on this half. Two of them did
affect it: RFC-0063 amends 0047 and 0052, and RFC-0121 amends 0026. Neither
RFC was back-pointed, and both back-pointers are now added.

## Verdicts

| RFC | `area:` | Verdict | Mechanical fixes made |
| --- | --- | --- | --- |
| 0004 Configuration model | `workspace-system-model`, confirmed | **amend** (brief item 2) | yes |
| 0015 RTOS orchestration via launch tree | `workspace-system-model`, confirmed | **keep** | yes |
| 0024 Multi-node workspace layout | `workspace-system-model`, confirmed | **amend** (brief item 1) | yes |
| 0025 Workspace layout by case | `workspace-system-model`, confirmed | **supersede-by 0121** (brief item 1) | yes |
| 0026 Example directory layout | `workspace-system-model`, confirmed | **amend** (brief item 3) | yes, including the RFC-0121 back-pointer |
| 0045 Unified boot-config resolution | `workspace-system-model`, confirmed (borders `runtime-api`) | **keep** (status: brief item 4) | yes |
| 0046 Launch-authoritative node identity | `workspace-system-model`, confirmed | **keep** (status: brief item 4) | yes |
| 0047 Unified sched-context binding | `workspace-system-model`, confirmed | **keep** (status: brief item 4) | yes |
| 0050 SystemModel | `workspace-system-model`, confirmed | **keep** (status: brief item 4) | yes |
| 0052 SystemModel to RTOS primitives | `workspace-system-model`, confirmed | **keep** (status: brief item 4) | yes |
| 0060 Launch toolchain three layers | `workspace-system-model`, confirmed | **keep** | yes |

Every one of the eleven now carries `last-reviewed: 2026-11-13`.

### RFC-0004: amend

RFC-0004 is `Stable`, and three of its central claims no longer describe the
tree.

- **The canonical-path banner.** It says the model is produced by `play_launch
  resolve … -o system_model.yaml` and consumed through `--model` / `MODEL` /
  `nros::main!(model = …)`, with `NROS_ALLOW_LEGACY_BAKE` as an escape. Since
  RFC-0063 the model is a build artifact that `nros sync` writes, and an entry
  names `launch = "bringup[:file]"`. `model =` is the deprecated arm
  (`packages/core/nros-macros/src/main_macro.rs:513`, `:590`).
  `NROS_ALLOW_LEGACY_BAKE` has no occurrence outside Markdown.
- **"`nros.toml` is retired".** It is still true of the Phase-172 manifest,
  but the name has two later meanings. The workspace-root search-path file
  (`[workspace] package_paths`, `[workspace.overrides]`) is accepted since
  phase-420 W6 (`packages/cli/nros-cli-core/src/orchestration/nros_config.rs:214-222`,
  where the root `nros.toml` is refused only when it is not search-path-only).
  The overlay `nros sync` writes beside the model is RFC-0060's 2026-10-10
  amendment.
- **§3 "Config in practice".** It says embedded network config lives in
  `[package.metadata.nros.deploy.<t>]`, and cites
  `examples/mps2-an385-baremetal/rust/talker-rtic`. No tracked `Cargo.toml`
  carries that table (`git grep -l metadata.nros.deploy -- '*/Cargo.toml'`
  finds none), and that example is not tracked. Network identity is now
  `[image.<id>] locator` plus `os_config = ["board-net.toml"]` in the bringup
  (for example
  `examples/mps2-an385-baremetal/c/action-client/src/baremetal_c_action_client_bringup/system.toml:17-28`),
  applied by `Config::from_image()`
  (`packages/boards/nros-board-mps2-an385/src/config.rs:403`).
- **§2, single-node `system.toml` optional and synthesised.** RFC-0121 D2/D3
  put `system.toml` only in a bringup and retire `synthesise_self_bringup`
  (RFC-0121 "Retirements"). RFC-0121 does not list RFC-0004 in its `amends`.
- **Pair 0004-0031 (tier 1):** complementary. §8 hands RMW selection to
  RFC-0031, and RFC-0031 does not restate the config homes.
- **Pair 0004-0109:** already resolved. §6 was replaced on 2026-10-24 and the
  changelog says so.
- **Pair 0004-0016 (tier 2):** the evidence row is RFC-0016 lines 17-18,
  which say `[node.rt]` lives in `nros.toml`. RFC-0004 §5/§7 say it lives in
  the standalone `config.toml`, and the board crates agree
  (`packages/boards/nros-board-freertos/src/config.rs:32-33`). The stale side
  is RFC-0016, which is in the platform-board-rtos slice (brief item 5).
- **Fixed:** a review note under the canonical-path banner (RFC-0063, the
  resolver, the two meanings of `nros.toml`, RFC-0121). A re-measurement
  note at the head of "Config in practice". A note in §7 on where `[node.rt]`
  is parsed. The phase-212 roadmap link now points at `archived/`. "See also"
  gains RFC-0063, RFC-0060 and RFC-0121. A changelog entry.
- **Not fixed:** the §3 ownership table, "Config in practice" and the §5
  `DeployOverlay` example are left as written. Rewriting them restates the
  configuration homes, which is a design text (brief item 2).

### RFC-0015: keep

The execution model it locks (Model 1: one RTOS task and one `Executor` per
sched task, one shared session per binary) is what the code does:
`run_sched_tasks` appears in 65 `.rs` files, `group_tasks` in 32 and
`resolve_sched_tasks` in 18. The superseded halves (the per-package
`nros.toml` manifests of §4, the `generate-main` orchestrator of §3 and §11)
are already fenced by banners.

- **Pair 0015-0032 (tier 2):** resolved. RFC-0032 says its pipeline supersedes
  §3/§11's `cargo nano-ros generate-main`. RFC-0015's own banner, "Entry-emit
  pipeline superseded by RFC-0032", says the same.
- **Fixed:** the canonical-path banner gains the RFC-0063 note. The phase-212
  banner's "Shared state → `[[shared_state]]`" now says it was removed by
  issue 0079 (resolved; `shared_state` has no occurrence in any `.rs` file).
  The "Rust caveat" now says issue #128 is resolved
  (`docs/issues/archived/0128-zephyr-entry-macro-no-params-tiers-lifecycle.md`,
  `status: resolved`).

### RFC-0024: amend

Pair 0024-0025 (tier 1) is the one this slice was asked to decide. The two
RFCs describe one phase-212 layout from two angles: 0024 the decisions, 0025
the five per-case file trees. The layout they describe has since been
replaced.

- RFC-0121 D1/D2 (phase-531): every project is a workspace, and `system.toml`
  lives only in a bringup. RFC-0024 §11.2 says a workspace with one Entry pkg
  and one Node pkg folds the bringup's `launch/` and `system.toml` into the
  Entry pkg. That is the shape D2 refuses.
- RFC-0065 D1 and RFC-0121 D6: the entry is generated under `build/` by
  `nros build`. §5 and §11.2 show a hand-written Entry pkg and a
  workspace-root `Cargo.toml`. `check-workspace-root-build-files` refuses the
  latter under `examples/{workspaces,templates}` (CLAUDE.md).
- `[deploy.<target>]` in §4, §5 and §11.2 is refused by name since phase-486
  W8 (RFC-0004 changelog, 2026-10-12).
- Phase-212 is archived. RFC-0121 does not list 0024 in its `amends`.

Much of 0024 is still the cited rationale for live code. It is named in 10
design documents (including ARCHITECTURE.md:339) and 25 times in 14
non-Markdown files: §2.4 in `packages/cli/nros-cli-core/src/builder/handoff.rs:6`
and `:346`, §6.3 in `builder/plan.rs:19`, §4 in `cmd/check_workspace.rs:153`
and `cmd/bringup.rs:11`, §11.1 in `cmd/emit_package_xml.rs:229`, and §11.6 in
`packages/api/nros/src/lib.rs:429`. Many of the surfaces it names still exist:
`nros plan` (`cmd/mod.rs:340`), `default_system`
(`orchestration/nros_config.rs:903`), `nros emit package-xml`
(`cmd/emit_package_xml.rs`).

- **Pair 0024-0065 (tier 2):** resolved. §2.4 and §9 carry the 2026-08-25
  amendment note that points at RFC-0065.
- **Fixed:** a review note under the §1 banners names each later RFC and what
  it changes, and lists the sections code still cites. The §3 citations
  now point at `docs/roadmap/archived/phase-212-…` and
  `packages/cli/colcon-cargo-ros2/` (was `nros-cli/packages/colcon-cargo-ros2/`).

### RFC-0025: supersede-by 0121

0025 is a by-case reference with no decision of its own. Its header says it
"matches design decisions in RFC-0024". Each of its five cases is now a
different shape.

- Cases 1 and 2 and the decision rule "Single-node → no bringup pkg" are the
  standalone shape RFC-0121 D2 retires and D8 migrates.
- Cases 3 to 5 show a hand-written Entry pkg (`cargo run -p linux_entry`) and
  a root `Cargo.toml`, which RFC-0065's generated entry replaces.
- Case 5's `#[nros::component(name = "talker")]` attribute never shipped. The
  node macro is `nros::node!` (`packages/core/nros-macros/src/lib.rs:238`).
- It is named in 4 design documents and in 5 code comments in 3 files
  (`cmd/bringup.rs:13`, `cmd/new_system.rs:5`, `:428`, `:443`,
  `orchestration/cargo_metadata_schema.rs:497`).
- **Fixed:** a review note under the header. The Case 1 `nros-build` path is now
  `packages/cli/nros-build` (was `packages/nros-build`, which does not exist).
  The `#[nros::component]` sketch is named in the note and not rewritten.

### RFC-0026: amend

Its rules hold: platform, then language, then example; RMW is a build choice,
not a directory; there are three README tiers; test-only binaries live under
`packages/testing/`. The text of the copy-out unit is out of date.

- RFC-0121's "Amendments to other RFCs" table says an example is a copy-out
  WORKSPACE, and a single-package example is one node package plus one
  bringup. 0026 had no back-pointer to it. The tree is mid-migration:
  `examples/linux/rust/talker` is already `src/{native_talker,native_talker_bringup}`.
  But 22 directories under `examples/` still hold a `system.toml` beside a
  `Cargo.toml` or `CMakeLists.txt` (for example
  `examples/rv-virt-threadx/rust/talker`). Phase-531 W6 is "in progress
  (2026-11-06)".
- Stale facts. "Its own `Cargo.toml` + `.cargo/config.toml`": no tracked
  `examples/**/.cargo/*` exists (0 files; RFC-0098 D1). The `--locked` shim
  `scripts/bin/cargo` does not exist (retired by phase-485 W4, RFC-0104 D9).
  The duplicated `NROS_RMW` / `NROS_RMW` names: the alias was `NANO_ROS_RMW`,
  now retired (`cmake/NanoRosRetiredNames.cmake:35`). The `ws-<topic>-<lang>`
  showcases: none remains in `examples/workspaces/`. The `-entry` sibling
  directories: none is tracked under `examples/`.
- **Fixed:** the changelog entry with the RFC-0121 back-pointer. Each stale
  fact above is corrected in place, with the current gate or file named.
- **Not fixed:** the "Canonical shape" section is not rewritten to the
  workspace shape (brief item 3).

### RFC-0045: keep

Phase-266 is archived as complete: W1 to W4 for Rust, and W5 to W7 for the
C/C++ `.nros_boot_config` blob. The resolver is
`ExecutorConfig::{resolve, resolve_with, try_resolve, try_resolve_with}`
(`packages/core/nros-node/src/executor/types.rs:658-687`). The blob
(`nros_boot_config`) appears in 31 source files. The design holds.

- **`area:` check:** this RFC is about how the configured identity reaches the
  session. It borders `runtime-api`. It stays in workspace-system-model,
  because its input is the bringup's configuration and RFC-0004 is its parent.
- **Fixed:** the precedence diagram's `[deploy.*]` is annotated with its
  replacement, `[image.<id>]` / `[host.<name>]`. A changelog entry records the
  resolver's location.
- One gap is recorded in its own 2026-09-21 amendment: a namespace reaches no
  C-ABI board runner. It is a known follow-on, not an open design question.

### RFC-0046: keep

It is implemented by phase-268 (archived). Its own banner says "Flip to Stable
with the ARCHITECTURE.md update when convenient." The single site is
`Executor::node_builder` (`packages/core/nros-node/src/executor/spin.rs:5739`),
and C++ reaches it at `packages/api/nros-cpp/src/lib.rs:3340`.

- **Fixed:** the two line cites in "One resolution site" (were
  `node_record.rs:270` and `nros-cpp/src/lib.rs:1007`), and a review line
  under the banner. The problem table's `emit_c.rs:106` is left, as the
  record of the state before the fix. That file is gone, and the C entry is
  now the template `packs/entry/c/node_body.jinja`.

### RFC-0047: keep

Phases 272 and 273 are archived. The surface is in the code:
`Node::create_callback_group`, `create_timer_in_group` and
`create_subscription_in_group` (`packages/core/nros-node/src/executor/node.rs:2062`,
`:2080`, `:2328`), and the group table `Executor::bind_group_sched`
(`spin.rs:3385-3400`). The manifest `callback_groups` key is refused at parse
(`orchestration/cargo_metadata_schema.rs:335-341`).

- **Pair 0017-0047 (tier 2):** complementary. The evidence row is 0047's own
  reconciliation note, which re-scopes `sched_context` to an intra-sched-task
  knob (RFC-0017's sporadic server). RFC-0015's banner says the same.
- **Pair 0047-0063 (133's):** RFC-0063 amends 0047 (its scheduling dims are
  authored in `system.toml`, never in a committed model). 0047 had no
  back-pointer. One is now added.
- **Fixed:** open questions 2 to 4 are marked answered from the code. The key
  sits under `[[component]]` (`packages/core/nros-orchestration-ir/src/lib.rs:708`).
  An unmapped group falls back to the default
  (`executor/tests.rs:10418`). The key is `(name, namespace, group)`
  (`spin.rs:3385-3400`). Question 1, the concurrency type, is still open: the
  C++ API accepts `CallbackGroupType`, but a `Reentrant` group still runs one
  callback at a time (`packages/api/nros-cpp/include/rclcpp/callback_group.hpp:37-39`).
  Cross-references gain RFC-0063 and RFC-0079. A changelog entry.

### RFC-0050: keep

Its central decisions hold, and the newer RFCs build on them: one variant per
model, the shared schema in ros-launch-manifest, semantics that ship with the
schema, and a parser that leaves execution modeling to each consumer. The
model crates are reached by git tag (`ros-launch-manifest-model` …
`tag = "v0.3.14"`, four manifests).

- **Pair 0050-0052 (tier 1):** complementary, not a merge candidate. 0050 is
  the cross-repository contract shared with play_launch. 0052 is nano-ros's
  consumer half, and 0050 already defers to it for "causality and execution
  modeling per consumer". Merging them would put nano-ros's RTOS mapper into
  the document play_launch reads as the shared design.
- **Fixed:** a review note before "References" (RFC-0060 names the producer
  and rules the model's content; RFC-0063 makes it a build artifact; RFC-0052
  is the consumer half). The first open question, the schema of `[deploy]`,
  is marked answered by its retirement into `[image.<id>]` / `[host.<name>]`.

### RFC-0052: keep

The mapper design holds. It was amended on 2026-11-05 for the sched-task
names (phase 536 W0), so it is a live document.

- **Pair 0022-0052 (tier 2):** complementary. The evidence row is 0052's
  "Kept: the other senses of tier" table, which keeps RFC-0022's entity-API
  tiers by name.
- **Pair 0052-0063 (133's):** RFC-0063 amends 0052 and states that it leaves
  0052's fail-loud contract unchanged. 0052 had no back-pointer.
- RFC-0060's 2026-10-10 amendment moves `execution.features`, the system-wide
  `[lifecycle]` table and the `[deploy.*]` build facts out of the model, into
  the `nros.toml` overlay. 0052's parity-gap table still gives those rows the
  verdict **model**.
- **Fixed:** a review note at the head of "Canonical path" records both
  rulings, marks the three parity rows superseded, and names the resolver.
  The table rows themselves are left as the 2026-07-17 analysis.

### RFC-0060: keep

`Stable`, reviewed 2026-11, and the newer RFCs cite it rather than amend it.

- **Pairs 0060-0091 and 0060-0111 (tier 2):** both are citations, not
  conflicts. RFC-0091 names RFC-0060 as the source of the repository's
  `## Amendment` convention. RFC-0111 says the entry framework never enters the
  model, per the 2026-10-10 amendment.
- **Pairs 0060-0063, 0060-0074 and 0060-0100 (133's):** they apply the
  amendment. None changes this verdict.
- **Fixed:** frontmatter gained `supersedes: []` / `superseded-by: null`.
  Decision 1 and "rlm stays vendored" said rlm is a submodule reached through
  `third-party/ros-launch-resolve/third-party/ros-launch-manifest`. Since
  phase-332 W2 it is a git-tag dependency, one tag across the tree (`v0.3.14`
  on 2026-11-13), and that nested path is gone. Both places now say so, and
  the publish decision is unchanged.

## 133's half: verdicts

RFC-0063, 0074, 0078, 0079, 0086, 0100, 0103, 0121, 0122 and 0125, reviewed by
133 in #3673. Each RFC's mechanical fixes are in its own
`## Review (2026-11-13, phase-555 W2)` section. This table is moved here from
133's working file so the area has one record. Item numbers in the reason
column refer to the brief below.

| RFC | title | verdict | reason |
| --- | --- | --- | --- |
| 0063 | The SystemModel is a build artifact | keep | Built and gated; only issue 1121's status was stale (fixed). |
| 0074 | Ingress budget: rate and burst | keep | Stable; renamed names match the code; open work is phase-519's deferred W1/W2. |
| 0078 | A WCET bound is declared per profile | keep | Built as written; status is Draft though all its work is archived (item 4). |
| 0079 | Priority is allocated, not authored | keep | Every port declares a plan; status is Draft though its phase is archived (item 4). |
| 0086 | Unified configuration, transport tenant | keep | D3-D6 built; two descriptors still in `config/` (item 5); deprecated features past their release (item 6). |
| 0100 | One sizing model | keep | Built and landed through Amendment 3; status is Draft (item 4). |
| 0103 | One owner per fact | amend | D7's generated-path table names two files the code does not write (item 7). |
| 0121 | Workspace-first project layout | keep | In flight under phase-531; frontmatter fixed. |
| 0122 | Behaviour bringups, witnesses, cadence CI | keep | In flight under phase-535; consistent with the tree. |
| 0125 | Topic visibility and locality | keep | Draft with no phase; W0 done (item 8). |

## Decision brief: workspace-system-model (for ruling)

Items 1 to 4 and 9 come from 142's half and items 5 to 8 from 133's. Item 4
merges the two halves' Draft-to-Stable question, and records where they disagree.


| # | Question | Options | Measured facts | Recommendation |
| --- | --- | --- | --- | --- |
| 1 | RFC-0024 and RFC-0025 describe the phase-212 layout, which RFC-0121 (with RFC-0065) has replaced. What happens to them? | (a) Archive both, superseded by RFC-0121. (b) Merge 0025 into 0024 and amend 0024. (c) Archive 0025, superseded by RFC-0121. Cut 0024 down to the rationale code still cites (§2 constraints as amended, §6.3 "cmake wins when the graph crosses languages", §4's bringup rules, §11.5 launch XML verbatim, §11.8 the custom spin loop), point it at RFC-0121 for the layout, and make it Stable. | 0024 is named in 10 design docs and 25 times in 14 non-Markdown files, for sections that still hold (§2.4 `builder/handoff.rs:346`, §6.3 `builder/plan.rs:19`, §4 `cmd/check_workspace.rs:153`). 0025 is named in 4 docs and 5 code comments in 3 files, all for file trees. RFC-0121 D2 refuses 0024 §11.2's fold-into-entry shape and 0025's "single node, no bringup". Neither RFC is in RFC-0121's `amends`. Phase-212 is archived. | **(c).** Archiving 0024 (option a) strands code comments that cite rationale which is still true. Merging (option b) keeps 0025's five obsolete trees alive. W3 adds `0024` to RFC-0121's `amends`, moves 0025 to `archived/` with a forward pointer, repoints its 5 code comments (to RFC-0121 or 0024 §4), and updates ARCHITECTURE.md:339-340. |
| 2 | RFC-0004 is `Stable`, and its §2 (single-node `system.toml` optional and synthesised), its §3 ownership table and "Config in practice", and its §5 `DeployOverlay` example no longer describe the tree. When is it restated? | (a) Restate now, against RFC-0121, RFC-0109 and RFC-0098. (b) Restate once, when phase-531 W7 retires the standalone shape. Until then, the review notes added here mark what moved. (c) Split it: RFC-0121 and RFC-0109 own the configuration homes, and 0004 keeps the §3.1 one-home rule, the §4 schema and the §10 parameter model. | 41 of 182 tracked `system.toml` files (22 under `examples/`) still sit beside a `Cargo.toml` or `CMakeLists.txt`, the shape RFC-0121 D8 migrates. Phase-531 W6 is in progress. No tracked `Cargo.toml` has a `[package.metadata.nros.deploy.*]` table, while `DeployOverlay` appears in 21 `.rs` files. Network identity is `[image.<id>] locator` plus `os_config` (`…/baremetal_c_action_client_bringup/system.toml:17-28`). | **(b)**, and in W3 add `0004` to RFC-0121's `amends`. Restating now would describe a tree in mid-migration, and would need restating again at W7. Option (c) is worth doing at that point, because 0004's §3.1 rule and §10 still hold and are cited. |
| 3 | RFC-0026's "Canonical shape" still describes a standalone package. RFC-0121 and RFC-0122 amend it to a copy-out workspace. When is the body rewritten? | (a) Rewrite now. (b) Rewrite when phase-531 W7 and phase-535 close. Until then, the changelog back-pointers (RFC-0121 added here, RFC-0122 already present) carry it. | The same 22 standalone directories as item 2. RFC-0065 also amends 0026 (its header, "a copied-out workspace now requires `nros build`"), but 0026 does not list it. 0121 and 0122 list 0026 in `amends`. | **(b).** In W3, write `0065` into the list of RFCs that amend 0026, wherever W1's vocabulary puts that list. |
| 4 | An implemented Draft whose phase is done: does it move to Stable? (Cross-area. The platform area's P4 proposes: Stable only when the body has no [OPEN] items and no parked waves.) | (a) Apply P4 as written. (b) Stable whenever the phase is done. (c) Leave every status to W1's normalisation. | **0045:** phase-266 archived complete; no open-question section; follow-on tracks are named as non-goals. **0046:** phase-268 archived; no open questions; its banner asks for the flip. **0047:** phases 272/273 archived; open question 1 (concurrency type) is still open. **0050:** phase-296 archived; two open questions remain (how much of layer 1 the embedded slice needs; whether `nros plan`, still a verb at `cmd/mod.rs:340`, folds into model consumption). **0052:** phase-296 archived; five open questions; amended 2026-11-05. **0024:** item 1. | **(a)**, for 142's RFCs: 0045 and 0046 go to Stable now. 0047 goes to Stable once question 1 is filed as an issue and the RFC points at it. 0050 and 0052 stay Draft until their open questions are answered or filed. Option (b) would mark 0052 Stable while it is still taking amendments. **133's half** asks the same of RFC-0078, 0079 and 0100 (all their phases archived) and recommends promoting all three although each keeps open follow-ups (0078: a clock-rate check; 0079: RTOS foreign-band enumerators, headroom policy, per-port overrides), on the precedent of RFC-0074, promoted with phase-519 open. That is option (b) for those three, so the halves disagree: one ruling on P4 decides all eight. Under (a), 0078, 0079 and 0100 stay Draft until their open items are filed as issues. |
| 9 | Cross-area, for the platform-board-rtos reviewer: RFC-0016 lines 17-18 say direct-mode scheduling config "now lives in `nros.toml` `[node.rt]` (RFC-0004 §7)". | A mechanical fix in that slice; or leave it. | RFC-0004 §5/§7 say `[node.rt]` lives in the standalone `config.toml`. `packages/boards/nros-board-freertos/src/config.rs:32-33` parses `[node.rt]` and `[node.rt.freertos]` from it. | Fix in the platform-board-rtos slice: `nros.toml` becomes the standalone `config.toml`. Not edited here, because the file is outside this slice. |

### Item 5 — RFC-0086 D3: the two platform descriptors left in `config/`

**Question.** D3 moves every platform descriptor from `config/<name>/` into
its crate. Two remain. Should they move, or should D3 say which stay?

**Options.**
- (a) Move `bare-metal` into `packages/platform/nros-baremetal-common/` and
  `generic` into a crate of its own (or into `nros-platform`).
- (b) Amend D3: a platform with no crate of its own keeps its descriptor in
  `config/`, and the search path keeps `config/` as a root.
- (c) Leave both as they are, undocumented.

**Measured facts.**
- `git ls-files | grep nros-platform.toml$`: five under
  `packages/platform/nros-platform-<name>/`, plus `config/bare-metal/` and
  `config/generic/`.
- `packages/platform/` has `nros-baremetal-common` but no
  `nros-platform-bare-metal` or `nros-platform-generic` crate.
- `nros-cli-core/src/orchestration/board_descriptor.rs:237` still cites
  `config/bare-metal/` as the descriptor spelling.

**Recommendation.** (b) now, since the two have no crate to live in; (a)
only if a crate is created for another reason.

### Item 6 — RFC-0086 D5: remove `nros-log`'s deprecated features?

**Question.** D5 says the old `nros-log` pick-one features are "deprecated
for one release". A release has shipped. Should they be removed now?

**Options.**
- (a) Remove the five feature families now, keeping a build error that
  names the knob for anyone who still sets one.
- (b) Keep them one more release.

**Measured facts.**
- The deprecation landed 2026-10-06 (`git log -S'DEPRECATED (issue 1037'
  -- packages/core/nros-log/Cargo.toml`).
- `nros-v0.6.0` was tagged 2026-10-14; seven `nros-0.6.0-nros*` builds
  followed, the latest on 2026-11-07.
- `packages/core/nros-log/Cargo.toml` lines 59-88 still define the features
  (for example `dynamic-loggers-0 = []`).
- RFC-0103's changelog (2026-10-14) chose to keep retired-name error tables
  for good, because "a dropped row makes an old name silently ignored, not
  refused".

**Recommendation.** (a), keeping the refusal, so the same rule as RFC-0103's
retired names applies.

### Item 7 — RFC-0103 D7: the generated-path table does not match the code

**Question.** D7 lists where generated output lives. Two rows name files the
code does not write. Amend the table to the code, or change the code?

**Options.**
- (a) Amend D7 (and D4's mention of `locations.cmake`) to the code: the cargo
  config is `nros-cargo.toml` in the image's entry directory, and the cmake
  locations are GLOBAL properties set by `cmake/NanoRosLocate.cmake`, with no
  file.
- (b) Change the code to D7's paths (`<build>/nros/cargo.toml`,
  `<build>/nros/locations.cmake`).

**Measured facts.**
- `nros-cli-core/src/builder/cargo_config.rs`: `pub const FILE_NAME: &str =
  "nros-cargo.toml"`, "inside the image's entry directory".
- `git grep locations.cmake -- cmake zephyr packages` has no hit.
  `cmake/NanoRosLocate.cmake` runs `nros locate --all --format cmake` once
  per configure and "keeps the answers in GLOBAL properties". It keeps D4's
  other promises (normal variables, never `CACHE`).
- The other D7 rows hold: `resolved.toml` (`resolve.rs`), models under
  `<build>/nros/models/<bringup>/`, sizing under `<build>/nros/sizing/`.
- RFC-0103 is `Stable`, and its changelog says phase-484 W1-W7 implemented
  D1-D9.

**Recommendation.** (a). The behaviour D4 wants (one ask per configure,
normal variables) is built; only the file D7 imagined is not, and nothing
reads one.

### Item 8 — RFC-0125: open its phase now?

**Question.** RFC-0125 has decisions (2026-11-10) and W0 done, but no phase.
Open one now, or wait for the rlm grammar?

**Options.**
- (a) Open the phase now with Tier 1 (item 5) first: per D2 and Decision 1 it
  needs only `[image.<id>] matched_only_wire`, not the rlm grammar.
- (b) Wait until rlm ships `visibility`/`closed`/`locality()`, then open the
  phase with W1.

**Measured facts.**
- `implements-tracked-by: []`; "Work items (phase to be opened)".
- W1 and W3 need rlm's grammar. W2 (Tier 1) is opt-in per image and uses
  zenoh-pico's existing write filter (`Z_FEATURE_INTEREST`, on for every
  target).
- `matched_only_wire` and a backend setting `from_intra_process` have no hit
  under `packages/`.
- The rlm design doc and the play_launch companion doc it builds on were not
  found in any local checkout (unverified whether they are published).
- `scripts/check-rfc-phase-tracking.py` asks every live RFC to name a phase
  or state `implementation: none`.

**Recommendation.** (a): open the phase with W2 first, and W1/W3 behind the
rlm release.

### What a ruling unblocks

- Item 1: in W3, move RFC-0025 to `archived/`, reduce RFC-0024, update
  RFC-0121's `amends`, repoint 5 code comments, and update ARCHITECTURE.md.
- Items 2 and 3: nothing changes now. A restatement follows phase-531 W7, and
  RFC-0121's `amends` gains `0004` in W3.
- Item 4: status flips for 0045 and 0046, one issue filed for RFC-0047's
  question 1, and the ARCHITECTURE.md alignment the AGENTS.md drift rule
  requires.
- Item 9: one line in RFC-0016, in the platform-board-rtos slice.
- Items 5 to 8: see each item's recommendation above.
