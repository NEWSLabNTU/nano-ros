
# Phase-555 W2 decision brief: runtime-api

[Phase-555](../phase-555-design-doc-review.md) W2 reviews each
area's RFCs. Mechanical fixes went straight into the review PR. Anything
that would change a design is here, as one brief for the area, for the user
to rule. Reviewer: the wtf session, 2026-11-13. All 26 RFCs of the slice
were read against their pairs and the code on the review branch.

## Verdicts

| RFC | Verdict | `area:` | Evidence |
| --- | --- | --- | --- |
| 0001 architecture overview | **keep** | confirmed | Directory map is gated (`check-package-directories`). The C API section named `nros_init()` and `nros_publisher_t`; fixed to `rclc_support_init`, `rcl_publisher_t` (`packages/api/nros-c/src/support.rs:246`, `publisher.rs:120`). |
| 0002 RT execution model | **amend** (done: header) | confirmed | `ReadySet`, `SchedContext`, `Trigger::AllOf`, overrun policy exist (`packages/core/nros-node/src/executor/ready_set/mod.rs`, `sched_context.rs`). Amended in substance by 0084, 0106, 0108, 0113, which it did not name; it now does. See Q1. |
| 0007 service QoS | **keep** | confirmed | `create_service`/`create_client` take `qos` (`packages/core/nros-rmw/src/traits.rs:1985,2048`). Landed by phase-193, now in `implements-tracked-by`; `supersedes: [0008]` added (0008 already said `superseded-by: 0007`). |
| 0008 service QoS gap | **archive** | confirmed | Already `Superseded` by 0007. W3 moves it to `archived/`. |
| 0010 zero-copy raw API | **keep** | confirmed | `try_loan`/`try_borrow` exist (`packages/core/nros-node/src/executor/handles.rs:1072,1857`). Backend matrix corrected: XRCE and Cyclone are C backends with `NULL` loan slots (`packages/rmw/xrce/nros-rmw-xrce/src/vtable.c:219`). Tracked by phase-99 (link added both ways). See Q7. |
| 0018 C++ API | **amend** | confirmed | Constraints stand (RFC-0096 restores them). `nros::` names (29 in the body) are deleted by RFC-0089. See Q3. |
| 0019 C thin-wrapper discipline | **amend** | confirmed | Principle in force (RFC-0089 "Governed by"). Target shape unbuilt: `rcl_subscription_t` still mirrors fields (`packages/api/nros-c/src/subscription.rs:98-123`); phase-122 left 122.3.b unchecked. See Q4. |
| 0020 thin-wrapper audit | **merge-into 0019** | confirmed | A 2026-04 audit record whose re-run now lives in `codebase-audit-checklist.md` C1 (line 133). Tracked by phase-83 (link added both ways). See Q5. |
| 0021 blocking API rules | **keep** | confirmed | Rule holds: `rclc_client_call` checks `in_dispatch` and spins (`packages/api/nros-c/src/service.rs:2445-2470`). Names corrected. Tracked by phase-82 (link added both ways). |
| 0022 entity API tiers | **keep** | confirmed | Builders exist (`packages/core/nros-node/src/executor/node.rs:1915,1929`), zoo gone. Built by phase-189, now tracked (links fixed). |
| 0036 ROS 2 divergences | **keep** | confirmed | Ledger path fixed (`docs/reference/api-parity-ledger/` is per-row files). RFC-0128 back-pointer added. See Q6. |
| 0037 Rust and C surfaces | **supersede-by 0089** | confirmed | Every listed C entry point is gone: 447 `rcl_*`/`rclc_*` exports and one `nros_*`. `NodeCtx` is `nros::Node` (`spin.rs:6531`). See Q2. |
| 0041 unified callback receive | **keep** | confirmed | `create_client_with_callback`, `create_action_client_with_callbacks` (`node.rs:2370,2475`). Phase-239 complete. |
| 0043 entry real-callback binding | **keep** | confirmed | Amended by phase-483 (header present). `amends: [0032]` added: it says it supersedes RFC-0032 section 8a's sub-decision. |
| 0044 rclcpp-faithful component | **amend** (done: header) | confirmed | Built (phase-242 closed). C++ names superseded: `ComponentNode` merged into `rclcpp::Node` (`packages/api/nros-cpp/include/rclcpp/node.hpp:94`). `amends: [0043]` added. See Q1. |
| 0057 ament-parity registration | **keep** | confirmed | `nros_components_register_node` and `rclcpp_components_register_node` exist (`cmake/NanoRosVerbs.cmake:648`, `cmake/NanoRosAmentSurface.cmake:409`). `amends: [0044]` added. |
| 0077 image runtime | **keep** | moved to `platform-board-rtos` | `nros_platform_panic` in the platform ABI (`packages/platform/nros-platform-api/include/nros/platform.h:922`), `panic =` in the board descriptor and `[image]` (`packages/cli/nros-cli-core/src/orchestration/facade.rs:1014`). It owns platform and board facts, not a user API. Built by phase-366, now tracked. See Q1. |
| 0084 readiness is pushed | **keep** | confirmed | Phase-397 DONE. See Q1. |
| 0089 ROS 2 API adoption | **keep** | confirmed | The governing API RFC; 0096 amends it (`amends: [0089]` added on 0096). |
| 0096 one freestanding rclcpp | **keep** | confirmed | Phase-442 complete. |
| 0102 logger hierarchy | **keep** | confirmed | Phase-479 complete. Uses RFC-0049's ladder for its knob; no conflict. |
| 0105 C parameter server | **keep** | confirmed | Phase-491 complete. Its `/parameter_events` default (on, as upstream) is for the C server object a user creates; RFC-0060's image default (off) is the layer above it. See Q1. |
| 0106 executor park | **keep** | confirmed | Built by phase-436 (now tracked) and phase-502. RFC-0113 amends D5. See Q1. |
| 0108 preemption levels | **amend** | confirmed | Phase-509 DONE. Status `Accepted` is outside the vocabulary. It amends RFC-0107, which is not on `main`. See Q1, Q8. |
| 0113 RT unified wait | **keep** | confirmed | Phase-521 complete. |
| 0128 per-node lifecycle | **keep** | confirmed | Ruled; phase-554 OPEN. |

## Pairs

- **T1 0007-0041.** Complementary: 0007 threads one profile per service, 0041 drains a client's reply into that profile's depth. Each now names the other.
- **0002-0089.** 0089 cites 0002 as the reason a wait-set of dynamic `Waitable`s is declined. No conflict.
- **0019-0032.** 0032 cites 0019 for binding by identity. No conflict.
- **0021-0041.** 0041 keeps `Promise` as 0021's primitive. No conflict.
- **0022-0037.** 0037 waited for 0022, which landed. 0037 is stale: Q2.
- **0022-0052.** 0052 lists 0022 as one sense of "tier" in a naming table. No conflict.
- **0032-0043.** 0043 supersedes 0032 section 8a's binding point. `amends: [0032]` added on 0043; 0032's back-pointer belongs to the build-toolchain-codegen slice.
- **0043-0044, 0044-0057.** Declared amendments, now in frontmatter.
- **0043-0089, 0044-0089.** 0089 keeps the declarative hook and deletes the separate node type. 0043 already said so; 0044 now does for C++.
- **0049-0102.** 0102 sizes a pool on 0049's ladder. No conflict.
- **0089-0096.** Declared amendment, now in frontmatter.

## Questions for the user

| Q | Question | Options | Measured facts | Recommendation |
| --- | --- | --- | --- | --- |
| 1 | Seven RFCs are built but not `Stable`. Flip them? | **(a)** flip in W3, each with its ARCHITECTURE.md section (README drift rule). **(b)** leave them Draft. | Built and archived: 0044 (phase-242 closed 2026-07-16), 0077 (phase-366 complete 2026-08-18), 0084 (phase-397 DONE 2026-10-24), 0105 (phase-491 complete 2026-10-12), 0106 (phase-436 DONE, phase-521 complete), 0108 `Accepted` (phase-509 DONE). 0002 is a research doc that the four later RFCs amend. | **(a)** for 0077, 0084, 0105, 0106 and 0108. 0044 flips after Q3's rewrite of its C++ example (`nros::ComponentNode`, `NodeHandle`, `NROS_COMPONENT` in section 1). 0002 stays Draft: it is a model plus RTOS research, and 0108/0113 carry the settled parts. |
| 2 | RFC-0037 records a C and Rust surface that no longer exists. Rewrite it, or retire it? | **(a)** rewrite to the current surface. **(b)** supersede by RFC-0089, archive it, and move RFC-0128's amendment of it into 0089. | C exports: 447 `rcl_*`/`rclc_*`, one `nros_*` (`nros_c_support_init_baked`). The ROS 2 side is recorded and checked in `docs/reference/api-surface/` and `api-parity-ledger/` (`just check api-parity`). 0036 says a hand-written catalog drifts; 0037 drifted the same way. | **(b).** The surface record it promised is now generated and gated. A rewrite would drift again. |
| 3 | RFC-0018's "API Mapping", "Detailed API Design" and "Usage Example" describe `nros::` C++ names RFC-0089 deleted. | **(a)** rewrite them to `rclcpp::`. **(b)** cut them, keep 0018 as the C++ constraints RFC (freestanding, no exceptions, typed FFI that bypasses nros-c), and point at 0089 for names. **(c)** merge into RFC-0096 and supersede 0018. | 0018 has 29 `nros::` names. RFC-0096 quotes 0018's freestanding section as its basis (0096 lines 148-169). The user headers are `packages/api/nros-cpp/include/rclcpp/`. | **(b).** The constraints and the FFI shape are 0018's own and are cited. The names belong to 0089, and upstream's own docs describe them. |
| 4 | RFC-0019's target C struct shape (`state` + `_opaque` only) was never built. Keep it as a target? | **(a)** keep, and open a phase for 122.3.b and the field-mirror entities. **(b)** drop universal rule 1, keep the principle (no logic or state machine in the wrapper), and record that mirrored registration fields are accepted. | `rcl_subscription_t` mirrors topic, type, hash, QoS and handle id (`packages/api/nros-c/src/subscription.rs:98-123`). Later phases added fields to these structs (`type_support` by phase-492 W8, `options` on `rcl_publisher_t` by phase-515 C6, `publisher.rs:125`). Phase-122 left 122.3.b unchecked. | **(b).** Users reach the structs through rclc's functions, not their fields (RFC-0089), so the ABI risk the target guarded against is a rebuild, which 0019's own "ABI compatibility" section already accepts. If (a), a phase is needed under the tracking ruling. |
| 5 | RFC-0020 (the 2026-04 audit and its checklist) beside RFC-0019 and audit category C1. | **(a)** keep both. **(b)** merge 0020's checklist into 0019, its method into `codebase-audit-checklist.md` C1, and supersede 0020. | Every citation names them as one rule: "RFC-0019/0020" in 0089 (lines 24, 1252, 1582) and phase-515. C1 (`docs/development/codebase-audit-checklist.md:133`) is where the periodic re-run lives now. | **(b).** One rule, one RFC; the method belongs with the audit that runs it. |
| 6 | RFC-0036 has no row for the graph services nano-ros leaves off by default. | **(a)** add rows now. **(b)** add them from phase-556's audit, as one systematic rule ("graph services are opt-in per image") pointing at RFC-0060, with ledger rows per name. | RFC-0060 states parameter services default off (`docs/design/0060-launch-toolchain-three-layers.md:52`). 0036 has no row for `/rosout`, `/parameter_events`, parameter services, `use_sim_time` or the undeclared-parameter reply. Phase-556 measured these. | **(b).** Phase-556 owns the measurement; 0036's own rule is that a systematic divergence is stated once. |
| 7 | RFC-0010's follow-ups (99.J raw-bytes examples, 99.K bench and book chapter, Cyclone `dds_loan_sample`) are in no open phase. | **(a)** open a phase. **(b)** record them as not planned in 0010. | Phase-99 closed "v1 complete". Cyclone and XRCE leave the loan slots `NULL` (`vtable.cpp:455`, `vtable.c:219`). 0010's perf table is an estimate nobody measured. | **(b)**, until a user needs measured zero-copy on a DDS backend. The tracking ruling is then met by phase-99 alone. |
| 8 | RFC-0108 amends RFC-0107, and 18 files under `packages/` cite RFC-0107, but RFC-0107 is not on `main`. | **(a)** land RFC-0107 from branch `docs/rfc-0107-shared-executor-core` (2971ea707ae9ea6cfe5d63099206a22b24c3ce65) as `Stable`, amended by 0108. **(b)** fold its surviving decisions into 0108 and rewrite the citations. **(c)** land it as a record only: `status: Superseded`, `superseded-by: [0108]`, straight into `archived/`, so the citations resolve to a historical document. | Provenance (checked 2026-11-13): one commit, 2971ea707a, 2026-10-19, by the agent session `018htu5MpsTGKW5cfLZz7FRX`, adding one 290-line RFC file under `docs/design/` (not on `main`). It was never a pull request (`gh pr list --state all --head docs/rfc-0107-shared-executor-core` is empty), so it was never reviewed. What landed anyway: its migration steps 1 (launch remaps as one flash table) and 2 (arena floor only for unknown demand), under issue 1971, which the code cites as "RFC-0107 step 1/2" (`nros-node/src/executor/launch_remap.rs`, `build/arena_floor.rs`, the entry codegen templates); and steps 5 and 6 as amended by RFC-0108 (phase-509 P9/P10). The branch's RFC says "Proposal only; nothing here is implemented" (2026-10-19). Phase-509 built its steps 4-6 as 0108 amends them. `git grep -l RFC-0107 -- packages` lists 18 files (`nros-node/src/executor/scheduler.rs`, `nros-cpp/src/lib.rs`, the entry codegen templates). | **(a).** The number is the citation target in code. Landing it restores the target with one file; (b) rewrites 18 source comments. Its status line is updated to what phase-509 built. |

## What the ruling then unblocks

- W3: the status flips of Q1 with their ARCHITECTURE.md sections; 0008 and,
  per Q2/Q5, 0037 and 0020 move to `archived/` with forward pointers.
- Q4 (a) or Q7 (a) each needs a new phase doc, per the tracking ruling.

## Ruling (user, 2026-11-14) and what W3 changed

Each question took the recommendation, except Q6. Before any RFC flipped to
Stable, its open questions and unbuilt work were filed as issues and the RFC
points at them.

- **Q1 (a).** Stable: RFC-0077, 0084, 0105, 0106 and 0108 (`Accepted` maps to
  Stable), each with its `ARCHITECTURE.md` line. RFC-0044 flipped after its
  section 1 example was rewritten to `rclcpp::Node`, `rclcpp::NodeHandle` and
  RFC-0057's `nros_components_register_node(... SHAPE rclcpp)`. RFC-0002
  stays Draft. Issues filed: 2541 (64-callback ceiling, from 0084), 2542
  (XRCE, uORB and bare metal do not attribute arrivals, from 0084 and 0106),
  2543 (the smoltcp deadline source never run on hardware, from 0106), 2544
  (EDF across levels and per-callback thresholds, from 0108), 2545 (callbacks
  to cores and the cross-image deadline split, from 0108). RFC-0044, 0077
  and 0105 had nothing open.
- **Q2 (b).** RFC-0037 superseded by RFC-0089 and archived; RFC-0128's
  amendment of it now amends RFC-0089.
- **Q3 (b).** RFC-0018's "API Mapping", "Detailed API Design", "Usage
  Example" and "Callback Pattern Comparison" cut, and its error-handling code
  and file list (both in deleted `nros::` names) replaced by pointers to
  RFC-0089 and `include/rclcpp/`.
- **Q4 (b).** RFC-0019's opaque-only target and the 122.3.b/c/d plan dropped;
  the rule keeps "no logic or state machine in the wrapper" and accepts
  mirrored registration fields.
- **Q5 (b).** RFC-0020's checklist merged into RFC-0019, its method into
  audit category C1; RFC-0020 superseded by RFC-0019 and archived.
- **0008** moved to `archived/`.
- **Q6, not the recommendation.** Per the default-services ruling (phase-556
  W13), graph interfaces are ON by default with a per-image
  `node_interfaces = "none"` switch. RFC-0036 states that once, points at
  RFC-0060 and phase-556, and has a row per name for what still diverges.
- **Q7 (b).** RFC-0010 records 99.J, 99.K and Cyclone `dds_loan_sample` as
  not planned until someone needs measured zero-copy on a DDS backend.
- **Q8.** Done already (#3755).
