---
id: 829
title: "Two `SYSTEM_DEFAULT` QoS presets ship under one meaning and disagree on
  depth — 1 in `nros-rmw`, 10 in the `nros::qos` façade, each with two callers"
status: resolved
type: bug
area: api, rmw
resolved_in: "issue-0829 sentinel implementation (2026-09-03)"
related: [phase-379, phase-376, issue-0160, issue-0088, issue-0240, issue-0241, issue-0823]
---

## What was wrong

One name, two queue depths. `QoSProfile::QOS_PROFILE_SYSTEM_DEFAULT`
(`nros-rmw/src/traits.rs`) was a concrete Reliable / Volatile / KeepLast
profile with **depth 1**; `nros::qos::SYSTEM_DEFAULT` (`api/nros/src/lib.rs`)
was `= DEFAULT`, **depth 10**. Depth is how many samples history queues before
dropping, so which spelling a caller reached decided its queueing behaviour.

Found by `qos_preset_parity`, a phase-379 W5 drift test written expecting to
*prove* the two copies agreed. It failed on its first run.

## Why neither number was the answer

It was never a 1-vs-10 vote. There were six spellings, and three of them
(`nros::qos::SYSTEM_DEFAULT`, the C ABI macro, and its Rust mirror) said
`= DEFAULT` — a third position, not a vote for 10: it says the constant
carries no meaning of its own.

Upstream's `rmw_qos_profile_system_default` names **no concrete policy at
all**. Every field is a sentinel meaning "let the RMW decide", and
`RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT` is `0`. What settles the issue is that
the two reference RMWs resolve that one sentinel to **different depths**:

* `rmw_cyclonedds_cpp` — `create_readwrite_qos` folds each sentinel by `case`
  fallthrough (RELIABLE / VOLATILE / KEEP_LAST / AUTOMATIC) and gives depth its
  own branch: `dds_qset_history(qos, DDS_HISTORY_KEEP_LAST, 1)`.
* `rmw_zenoh_cpp` — `QoS::QoS()` fills `default_qos_` from
  `RMW_ZENOH_DEFAULT_HISTORY_DEPTH`, which is **42**, over a comment stating
  the contract outright: *"If the depth field in the qos profile is set to 0,
  the RMW implementation has the liberty to assign a default depth."*

One sentinel, two answers, both correct. So no baked number could be right, and
the fix was to carry the absence and resolve it per backend — recommendation
(A) of the 2026-09-03 study.

## What the sentinel means on each backend

Researched before implementing, because "what does this middleware default to"
turned out to differ from "what does the corresponding upstream RMW do", and
interop with a ROS peer makes the second one the requirement.

* **Cyclone DDS 0.10.5** — a fresh `dds_qos_t` carries nothing
  (`ddsi_xqos_init_empty`, `dds_qos.c:54-59`); defaults arrive at entity
  creation from `const` tables in `ddsi_plist.c` (`ddsi_default_qos_reader:3442`,
  `_writer:3490`). Reliability there is **asymmetric per the DDS spec** — reader
  BEST_EFFORT (`:3470`), writer RELIABLE — where `rmw_cyclonedds_cpp`
  deliberately picks RELIABLE for both. `KEEP_LAST` with depth 0 is **rejected,
  never clamped** (`validate_history_qospolicy`, `ddsi_plist.c:2603-2604`), so
  a sentinel depth reaching `dds_qset_history` raw would have failed create with
  `BAD_PARAMETER`. No sentinel value exists in Cyclone's own enums; the only way
  to say "unstated" is to not call `dds_qset_*`.
* **Micro XRCE-DDS Client 3.0.1** — no client-side default at all, and depth 0
  *is already* the sentinel on the wire: `optional_history_depth = qos.depth ==
  0 ? false : true` (`create_entities_bin.c:148`), so the field is simply absent
  from the CREATE submessage and the Agent supplies its own.
* **zenoh-pico 1.7.2 + our shim** — zenoh-pico has no history-depth concept on
  publishers or subscribers. History/depth/reliability/durability are
  **discovery metadata only** on this backend: formatted into the
  liveliness-token keyexpr (`keyexpr.rs`) and otherwise discarded. The receive
  queue is always the build-time `SUBSCRIBER_RING_DEPTH` (default 4).

## The three resolutions that already existed, and disagreed

The sentinel already reached code, and every site folded it differently:

| site | `reliability == 0` became |
| --- | --- |
| `qos_from_cffi` (`nros-rmw-cffi/src/rust_adapter.rs`) | `Reliable` |
| `xrce_map_qos` (`nros-rmw-xrce/src/session.c`) | `UXR_RELIABILITY_RELIABLE` |
| `make_dds_qos` (`nros-rmw-cyclonedds/src/qos.cpp`) | **`DDS_RELIABILITY_BEST_EFFORT`** |

Cyclone was the odd one out, and it is the backend that meets real ROS peers —
picking the less safe of the two. Reachable from a zero-filled or hand-rolled
C `rmw_qos_profile_t`. Fixed as its own commit, since it is a defect
independently of the sentinel work.

## Resolution

`SYSTEM_DEFAULT` now carries upstream's sentinel meaning end to end.

* `QoSHistoryPolicy` / `QoSReliabilityPolicy` / `QoSDurabilityPolicy` grew a
  `SystemDefault` variant (listed first, matching rclrs; not `#[default]`, so
  `QoSProfile::default()` still means `QOS_PROFILE_DEFAULT`).
  `DEPTH_SYSTEM_DEFAULT` is `0`; `QoSLivelinessPolicy::None` was already the
  liveliness sentinel.
* `QOS_PROFILE_SYSTEM_DEFAULT` is all sentinel, and `nros::qos::SYSTEM_DEFAULT`
  aliases it instead of restating a second profile.
* `required_policies` stopped starting from `QoSPolicyMask::CORE`
  unconditionally — a sentinel-valued policy declines its bit, exactly as a zero
  `deadline_ms` already did. Without this a `SYSTEM_DEFAULT` profile would have
  been rejected as `IncompatibleQos` for requesting nothing.
* Each backend resolves at its create entry via
  `QoSProfile::resolve_system_default`, **before anything is derived from the
  QoS** — load-bearing on the zenoh path, where the profile is serialised into
  the liveliness-token keyexpr a ROS peer parses.

| backend | resolves `SYSTEM_DEFAULT` to |
| --- | --- |
| cyclonedds | RELIABLE / VOLATILE / KEEP_LAST / **depth 1** — mirroring `create_readwrite_qos` |
| zenoh | RELIABLE / VOLATILE / KEEP_LAST / **depth `SUBSCRIBER_RING_DEPTH`** (4) — the number the shim actually enforces, not upstream's 42, because advertising a depth we cannot honour is the defect this issue is about |
| xrce | RELIABLE / VOLATILE / KEEP_LAST / **depth left at 0** — the client already encodes exactly this and the Agent's DDS layer resolves it |

**No ABI break.** No struct layout moved and no policy value was renumbered.
`NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT` stopped aliasing `_DEFAULT` and became the
all-zero initialiser — a change to the macro's expansion, visible only on
recompile, and nothing in the tree used it. The user-facing `nros_qos_t`
(`nros-c`) keeps its pre-phase-376 dense numbering where
`NROS_QOS_RELIABILITY_BEST_EFFORT = 0`, so it is deliberately left concrete and
the sentinel stays an RMW-layer concept. That is a real remaining divergence,
recorded rather than fixed: making `nros_qos_t` carry a sentinel would mean
renumbering a user-facing C enum.

`qos_preset_parity` was updated, not deleted: its pinned-divergence test became
`system_default_states_nothing_on_every_field`, which asserts the SHAPE (every
field a sentinel) and that `SYSTEM_DEFAULT != DEFAULT` — the aliasing being the
older half of the defect.

## Not carried over

* **The QoS-override path still cannot express `system_default`.**
  `decode_qos_override_value` (`nros-rmw/src/traits.rs`) is a bool-style
  `if value == 0 {…} else {…}` per policy, and its encoder
  (`nros-orchestration-ir/src/qos_override.rs`) accepts only the two concrete
  spellings, on a numbering that is NOT the C ABI's. Adding a third code means
  moving both ends together; it is a separate change.
* **The zenoh backend's `CORE` mask over-promises.**
  `shim/session.rs` advertises RELIABILITY / DURABILITY_VOLATILE / HISTORY /
  DEPTH as honoured, but all four are discovery metadata on that backend. So
  `validate_against` accepts a profile the backend then ignores — the silent
  downgrade the mask exists to prevent. Worth its own issue.
* **Whether a ROS `rmw_zenoh_cpp` peer's QoS matching considers the `depth`
  field of the liveliness-token keyexpr.** History/depth is not an RxO policy in
  DDS, which is why advertising our real 4 should be safe — but that is
  reasoning, not a measurement.
* **Line numbers for the upstream files** (`rmw/qos_profiles.h`, `rmw/types.h`,
  `rmw_cyclonedds_cpp/src/rmw_node.cpp`, `rmw_zenoh_cpp/src/detail/qos.cpp`).
  No ROS on the study host; the code quoted above is verbatim from the branch
  named, but the numbering is not trustworthy. Function and macro names are the
  stable anchors.
