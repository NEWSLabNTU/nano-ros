---
id: 829
title: "Two `SYSTEM_DEFAULT` QoS presets ship under one meaning and disagree on
  depth — 1 in `nros-rmw`, 10 in the `nros::qos` façade, each with two callers"
status: open
type: bug
area: api, rmw
related: [phase-379, phase-376, issue-0160, issue-0088, issue-0240, issue-0241, issue-0823]
---

## Problem

The same profile is defined twice, with different depths:

| | value | callers |
| --- | --- | ---: |
| `QoSProfile::QOS_PROFILE_SYSTEM_DEFAULT` (`nros-rmw/src/traits.rs:751`) | Reliable, Volatile, KeepLast, **depth 1** | 2 |
| `nros::qos::SYSTEM_DEFAULT` (`api/nros/src/lib.rs:733`) | Reliable, Volatile, KeepLast, **depth 10** | 2 |

Depth is not cosmetic — it is how many samples the history queues before
dropping. A publisher created through the façade queues ten; the same profile
name reached through `QoSProfile` queues one.

Neither is a typo of the other: the façade's is `..DEFAULT` (depth 10) and the
`nros-rmw` one is an explicit `build(..., 1)`. Two people wrote two constants.

Found by a drift test added in phase-379 W5 while giving the presets their
rclrs-shaped crate-level names. The test was written expecting to *prove* the
two copies agreed; it failed on the first run.

## Neither matches upstream, which is the harder half

`rmw_qos_profile_system_default` does not name concrete policies at all:

```c
static const rmw_qos_profile_t rmw_qos_profile_system_default = {
  RMW_QOS_POLICY_HISTORY_SYSTEM_DEFAULT,
  RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT,
  RMW_QOS_POLICY_RELIABILITY_SYSTEM_DEFAULT,
  RMW_QOS_POLICY_DURABILITY_SYSTEM_DEFAULT,
  ...
};
```

Every field is a *sentinel* meaning "let the RMW decide". Ours are concrete on
both sides, so `SYSTEM_DEFAULT` currently means "a profile someone picked",
not "the implementation's own default" — which is what a ported ROS 2 node
reading the name will assume.

So there are two questions, and only the first is a bug fix:

1. **Which depth wins?** The two copies must not disagree.
2. **Should the profile mean what upstream means?** That needs a sentinel
   concept (`SYSTEM_DEFAULT` as a distinct policy value) which the QoS repr
   does not have, and which reaches the C ABI's `nros_qos_t`.

## Why it was not decided in passing

Both spellings have exactly two callers, so neither is dominant and there is no
"obviously the live one". Picking the wrong depth silently changes queueing
behaviour for whichever set of callers loses — the sort of change that surfaces
as a dropped-sample bug three phases later, not as a test failure.

## Current state, and the guard that now exists

`packages/api/nros/src/lib.rs` gained the eight crate-level `QOS_PROFILE_*`
consts (rclrs parity — the names always matched, only the path did not) as
ALIASES of the `QoSProfile` associated consts. A test, `qos_preset_parity`,
asserts the façade's `nros::qos::*` module agrees with them.

Four of the five agree and are asserted. `SYSTEM_DEFAULT` is asserted at its
CURRENT divergent values with a pointer to this issue, so the known gap is
recorded and any FURTHER drift still fails. It is a pinned bug, not a passing
test.

## Direction

* Decide the depth, update one side, delete the duplicate definition so the
  façade aliases rather than restates (the other four already could).
* Separately: decide whether `SYSTEM_DEFAULT` should carry upstream's
  sentinel meaning, which is an RFC-0036 divergence-row question and reaches
  the C ABI.

## Investigation, 2026-09-03 — what should `SYSTEM_DEFAULT` mean?

Research only; nothing in this pass changed behaviour, and `qos_preset_parity`
still pins the divergence.

### It is not two definitions — it is six spellings, and the count runs the other way

| spelling | value | site |
| --- | --- | --- |
| `QoSProfile::QOS_PROFILE_SYSTEM_DEFAULT` | Reliable / Volatile / KeepLast / **1** / Automatic | `packages/core/nros-rmw/src/traits.rs:751` |
| `QoSProfile::system_default()` | aliases the above | `packages/core/nros-rmw/src/traits.rs:873` |
| `nros::QOS_PROFILE_SYSTEM_DEFAULT` | aliases the above | `packages/api/nros/src/lib.rs:936` |
| `nros::qos::SYSTEM_DEFAULT` | `= DEFAULT`, depth **10** | `packages/api/nros/src/lib.rs:990` |
| `NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT` (C ABI) | `= NROS_RMW_QOS_PROFILE_DEFAULT`, depth **10** | `packages/core/nros-rmw-abi/include/nros/rmw_entity.h:735` |
| `NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT` (Rust mirror of the C ABI) | `= NROS_RMW_QOS_PROFILE_DEFAULT`, depth **10** | `packages/rmw/cffi/src/lib.rs:366` |

Three of the six say "SYSTEM_DEFAULT is literally DEFAULT" — which is a third
position, not a vote for 10: it says the constant carries no meaning of its own
at all.

**Nothing in the tree creates an entity with any of them.** `grep` over
`packages/`, `examples/` and `book/` finds only the definition sites, the alias
sites and `qos_preset_parity`. So whichever depth is chosen, no in-tree caller
changes behaviour; the entire cost falls on a ported ROS 2 node reading the
name, which is exactly the reader upstream wrote the sentinel for.

### 1. Upstream — the sentinel is resolved by the RMW, and the two reference RMWs resolve it to different depths

`rmw_qos_profile_system_default` (ros2/rmw, `humble`,
`rmw/include/rmw/qos_profiles.h`) is all sentinel, and
`RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT` is spelled `enum
{RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT = 0};` in `rmw/include/rmw/types.h`. Depth
upstream is `size_t`; ours is `uint16_t` (`rmw_entity.h`), which is fine — 0 is
0 in both.

What the two reference RMWs do with it:

* **`rmw_cyclonedds_cpp`** (`rmw_cyclonedds_cpp/src/rmw_node.cpp`,
  `create_readwrite_qos`) folds each sentinel into a concrete DDS value by
  `case` fallthrough: `HISTORY_SYSTEM_DEFAULT` joins `KEEP_LAST`;
  `RELIABILITY_SYSTEM_DEFAULT` joins `RELIABLE`; `DURABILITY_SYSTEM_DEFAULT`
  joins `VOLATILE`; `LIVELINESS_SYSTEM_DEFAULT` joins `AUTOMATIC`. Depth has its
  own branch: `if (qos_policies->depth == RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT)
  { dds_qset_history(qos, DDS_HISTORY_KEEP_LAST, 1); }`. So under Cyclone,
  `rmw_qos_profile_system_default` resolves to **RELIABLE / VOLATILE /
  KEEP_LAST(1) / AUTOMATIC**.
* **`rmw_zenoh_cpp`** (`rmw_zenoh_cpp/src/detail/qos.cpp`,
  `QoS::best_available_qos`) substitutes `default_qos_` field by field, and
  `default_qos_` is built in `QoS::QoS()` from file-local defines:
  `RMW_ZENOH_DEFAULT_HISTORY KEEP_LAST`, `RMW_ZENOH_DEFAULT_RELIABILITY
  RELIABLE`, `RMW_ZENOH_DEFAULT_DURABILITY VOLATILE`, and
  `#define RMW_ZENOH_DEFAULT_HISTORY_DEPTH 42`. The comment above that define
  states the contract in so many words: *"If the depth field in the qos profile
  is set to 0, the RMW implementation has the liberty to assign a default
  depth."*

**That is the answer to the framing question.** Cyclone resolves the sentinel to
depth 1; zenoh resolves it to depth 42. There is no concrete number that is
"what `SYSTEM_DEFAULT` means" — the meaning is *per backend*, which is the whole
point of the name. Our `1` is the right answer for one of our three backends by
coincidence, and our `10` is the right answer for none.

(No ROS is installed on this host — `/opt/ros` does not exist — so these four
files were read from `raw.githubusercontent.com` at the branch named. The code
is verbatim; the **line numbers the fetch reported are renumbered and are not
quoted here**. The function/macro names are the stable anchors. `nros-sdk-index.toml:842`
names `ros-humble-rmw-zenoh-cpp` as the documented default distro, and
`rmw_zenoh` has no `humble` branch, so its `rolling` was read.)

### 2. Per backend — does the middleware itself have a default?

#### Cyclone DDS 0.10.5 — yes, and depth 0 is a hard error

Version verified at `third-party/dds/cyclonedds/CMakeLists.txt:13`
(`project(CycloneDDS VERSION 0.10.5 …)`) and `package.xml:5`. (`CHANGELOG.rst`
stops at 0.7.0 — do not version from it.)

* A fresh `dds_qos_t` carries **nothing**: `dds_create_qos()` calls
  `ddsi_xqos_init_empty`, which only zeroes the `present` bitmask
  (`src/core/ddsc/src/dds_qos.c:54-59`,
  `src/core/ddsi/src/ddsi_plist.c:3422-3428`). There is no
  `ddsi_xqos_init_default` in 0.10.5. "Unset" is a *presence* encoding, not an
  in-band value.
* Defaults arrive at entity creation, from four `const` tables in
  `src/core/ddsi/src/ddsi_plist.c` — `ddsi_default_qos_reader:3442`,
  `_writer:3490`, `_topic:3535`, `_publisher_subscriber:3571` — merged by
  `ddsi_xqos_mergein_missing` in the order caller → pub/sub → topic → per-kind
  table (`src/core/ddsc/src/dds_reader.c:658-665`, mirrored at
  `dds_writer.c:422-429`).
* The values: history `KEEP_LAST` depth **1** for reader (`:3454-3455`), writer
  (`:3502-3503`) and topic (`:3547-3548`); durability `VOLATILE` for all three
  (`:3448`, `:3496`, `:3541`); reliability **asymmetric per DDS spec** —
  reader `BEST_EFFORT` (`:3470`), writer `RELIABLE` with
  `max_blocking_time = DDS_MSECS(100)` (`:3523-3524`).
* **`KEEP_LAST` with depth 0 is rejected, never clamped**: the setter validates
  nothing (`src/core/ddsc/src/dds_qos.c:162-169`), and
  `validate_history_qospolicy` returns `DDS_RETCODE_BAD_PARAMETER` for
  `kind == DDS_HISTORY_KEEP_LAST && depth < 1`
  (`src/core/ddsi/src/ddsi_plist.c:2603-2604`), reached from every entity-create
  path via `ddsi_xqos_valid` (`dds_reader.c:671`, `dds_writer.c:435`,
  `dds_topic.c:499`, and `dds_entity.c:755` for a live `dds_set_qos`).
* **No sentinel exists.** Every kind enum in
  `src/core/ddsc/include/dds/ddsc/dds_public_qosdefs.h` is closed and spec-only
  — `dds_history_kind:88-93` has exactly `KEEP_LAST`/`KEEP_ALL`,
  `dds_reliability_kind:116-121` exactly `BEST_EFFORT`/`RELIABLE`. A repo-wide
  grep for `SYSTEM_DEFAULT`/`system_default` over `src/` returns only IDL
  `@try_construct(USE_DEFAULT)` hits, unrelated to QoS. The only way to say "I
  did not state this" is to not call `dds_qset_*`.
* **No runtime override.** No `DDS_DEFAULT_*` macros exist; the tables are
  `const` (`src/core/ddsi/include/dds/ddsi/ddsi_xqos.h:349-352`); `CYCLONEDDS_URI`
  is read once at `src/core/ddsc/src/dds_participant.c:110` into `ddsi_config`,
  which has no QoS-default members. `ExplicitlyPublishQosSetToDefault`
  (`ddsi_cfgelems.h:918-920`) decides only whether a default-valued policy is
  put *on the wire* during SEDP, never what the local entity uses.

Consequence for us: if `depth == 0` ever reached `make_dds_qos`, the Cyclone
entity create would fail `BAD_PARAMETER`. Today it can, because
`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/qos.cpp:46-50` passes
`src->depth` to `dds_qset_history` raw.

#### Micro XRCE-DDS Client 3.0.1 — no client-side default at all, and depth 0 *is already* the sentinel on the wire

Version at
`packages/rmw/xrce/xrce-sys/micro-xrce-dds-client/CMakeLists.txt:95`
(micro-CDR 2.0.2 at `micro-cdr/CMakeLists.txt:51`).

* QoS is a four-field struct on the BIN path: `uxrQoS_t { durability,
  reliability, history, depth }`
  (`micro-xrce-dds-client/include/uxr/client/core/session/create_entities_bin.h:56-62`),
  taken by value by `uxr_buffer_create_datawriter_bin` (`:171-178`) and
  `..._datareader_bin` (`:197-204`). The XML path (`create_entities_xml.h:146-152`)
  forwards an opaque string the client never parses
  (`src/c/core/session/create_entities_xml.c:173-176`); the REF path is a name only.
* **`depth == 0` clears the optional field**:
  `datawriter.qos.base.optional_history_depth = qos.depth == 0 ? false : true;`
  (`src/c/core/session/create_entities_bin.c:148`, reader identically at `:213`).
  The depth is then simply absent from the CREATE submessage and the Agent
  supplies its own. No error, no clamp, no signal to the caller.
* There is **no `UXR_QOS_DEFAULT`** or any client-side profile constant: a grep
  for `uxrQoS`/`uxr_qos` across `include/` and `src/` hits only
  `create_entities_bin.{h,c}`. Every default is the Agent's.
* XRCE's own reliability is a *stream* property, not a DDS policy —
  `uxr_create_output_reliable_stream(session, buffer, size, history)`
  (`include/uxr/client/core/session/session.h:370-374`), where `history` splits
  the buffer into that many slots and doubles as the un-ACKed send window
  (`src/c/core/session/stream/common_reliable_stream_internal.h:33-46`,
  `src/c/core/session/stream/output_reliable_stream.c:75-76`). Unrelated to, and
  never derived from, the DDS `depth`. Ours defaults to 16
  (`packages/rmw/xrce/nros-rmw-xrce/src/internal.h:129-134`). A stream
  `history == 0` is an unguarded divide-by-zero — the source carries only the
  comment `// assert for history (must be 2^)`
  (`src/c/core/session/stream/output_reliable_stream.c:25`).

Our backend: `xrce_map_qos`
(`packages/rmw/xrce/nros-rmw-xrce/src/session.c:199-218`) uses reliability
(`:211-213`), durability (`:208-210`), history (`:214-215`) and copies depth
verbatim (`:216`); `deadline_ms`, `lifespan_ms`, `liveliness_kind` and
`liveliness_lease_ms` are **ignored** — the client hardcodes
`optional_deadline_msec`/`optional_lifespan_msec` to false
(`create_entities_bin.c:144-145`) and `OBJK_Endpoint_QosBinary`
(`include/uxr/client/core/type/xrce_types.h:510-522`) has no liveliness field.
Services fare worse still: `service.c:159-165` and `:590-596` map a profile that
`uxr_buffer_create_requester_bin`/`_replier_bin` then discard outright —
`create_entities_bin.c:268` and `:308` are literally `(void) qos;`.

#### zenoh-pico 1.7.2 + our shim — the "middleware" IS our shim, and its depth is a compile-time constant

**zenoh-pico has no history-depth concept at all** on core publishers or
subscribers, and no aggregate "default QoS profile" struct. What it has is
priority (`Z_PRIORITY_DATA` = 5), congestion control (`DROP` for push), and an
unstable transport-level `z_reliability_t` (`RELIABLE` = 0) that selects a frame
channel and enforces monotonic sequence numbers without repair — i.e. not DDS
RELIABLE. It does ship a TRANSIENT_LOCAL analogue (advanced-publisher cache /
advanced-subscriber history), off by default upstream and **hard-compiled-out by
nano-ros** at `packages/rmw/zenoh/nros-zpico-build/src/lib.rs:311,313`. So on
this path there is no middleware default to defer to.

Our own `supported_qos_policies` says as much and says what we do instead:
*"zenoh-pico's wire protocol has no native DDS QoS, so the shim emulates
everything … Durability VOLATILE / History / Depth honoured at the subscriber
buffer level (CORE)"*
(`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs:1245-1268`). For this
backend, "whatever the middleware chooses" means *whatever our shim's own
constants are*.

And those constants do not come from the profile. `history`, `depth`,
`reliability` and `durability` are **discovery metadata only** on this backend:
they are formatted into the liveliness-token keyexpr (`keyexpr.rs:185-214`) and
otherwise discarded. `QoSProfile::depth` is read in exactly two places in the
whole backend — `keyexpr.rs:208` (the token string) and `keyexpr.rs:356` (a unit
test); `reliability` and `durability` appear only in `keyexpr.rs` and in
`shim/mod.rs`'s tests. The subscriber declare passes no options at all
(`shim/subscriber.rs:711-719` — four arguments, no QoS). The receive queue is
always the build-time `SUBSCRIBER_RING_DEPTH` (`ZPICO_SUBSCRIBER_RING_DEPTH`,
**default 4**, floor 1 — `packages/rmw/zenoh/nros-rmw-zenoh/build.rs:37`, emitted
at `:96`), statically allocated per subscriber (`shim/subscriber.rs:51-57`,
`:192-201`), and overflow drops at `shim/subscriber.rs:1538`
(`if tail - head >= SUBSCRIBER_RING_DEPTH`). `depth == 0` gets no handling
anywhere and is indistinguishable from any other value.

So the zenoh resolution of `SYSTEM_DEFAULT` is not a choice we would be making —
it is 4, today, for every profile. (See the spin-off note below: that is the
`CORE` mask over-promising, independently of 0829.)

### 3. Is the sentinel expressible at our seam?

Partly — and the interesting finding is that **it is already there, already
resolved, and already resolved to three different answers.**

**The C RMW ABI carries it.** phase-376 W5/B2 moved the policy values to
upstream's numbering, which means `0` is `SYSTEM_DEFAULT` on every policy:
`NROS_RMW_RELIABILITY_SYSTEM_DEFAULT` / `_DURABILITY_` / `_HISTORY_` at
`packages/core/nros-rmw-abi/include/nros/rmw_entity.h:70,78,84`, and
`NROS_RMW_LIVELINESS_SYSTEM_DEFAULT = 0` at `:266`. The header's own preamble
(`:44-56`) spells out that this renumbering was done precisely because these
values cross the ABI to a ROS peer. Depth has no named sentinel, but `0` is
free: Cyclone rejects it, XRCE reads it as "unstated", and upstream defines it
as the sentinel.

**The Rust `QoSProfile` cannot carry it.** `QoSHistoryPolicy` (`traits.rs:333-339`),
`QoSReliabilityPolicy` (`:343-352`) and `QoSDurabilityPolicy` (`:356-362`) are
two-variant enums with no `SystemDefault`. And the mechanism that *would* carry
"I did not request this" refuses to: `QoSProfile::required_policies`
(`traits.rs:1626-1655`) starts from `QoSPolicyMask::CORE` unconditionally, and
`CORE` is `RELIABILITY | DURABILITY_VOLATILE | HISTORY | DEPTH`
(`traits.rs:1591-1593`). The pattern for "not requested" already exists in that
same function — a zero `deadline_ms`/`lifespan_ms`/`liveliness_lease_ms` and
`LivelinessPolicy::None` all decline to set their bit — it just was never
extended to the four CORE policies.

**The user-facing C API cannot carry it either, and disagrees about what `0`
means.** `nros-c`'s `nros_qos_t` uses a separate, still-dense vocabulary:
`NROS_QOS_RELIABILITY_BEST_EFFORT = 0` / `_RELIABLE = 1`
(`packages/api/nros-c/include/nros/nros_generated.h:529-533`), likewise
durability `:543-547` and history `:557-561`. So a `memset`-zeroed `nros_qos_t`
means BEST_EFFORT at the user API while a `memset`-zeroed `rmw_qos_profile_t`
means SYSTEM_DEFAULT one layer down, and `nros_qos_t::to_qos_settings`
(`packages/api/nros-c/src/qos.rs:156-172`) matches exhaustively on two variants
with nowhere for a third to go.

**Three live resolutions, and they do not agree.** The sentinel already reaches
code, and every site folds it differently:

| site | `reliability == 0` becomes | `depth == 0` becomes |
| --- | --- | --- |
| `qos_from_cffi` — `packages/rmw/cffi/src/rust_adapter.rs:258-263` | `Reliable` (`_ =>` arm, commented as deliberate) | passed through raw (`:273`) |
| `make_dds_qos` (cyclone) — `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/qos.cpp:25-30` | **`DDS_RELIABILITY_BEST_EFFORT`** (the `?:` else-branch) | passed to `dds_qset_history` raw (`:46-50`) ⇒ `BAD_PARAMETER` at create |
| `xrce_map_qos` — `packages/rmw/xrce/nros-rmw-xrce/src/session.c:211-213` | `UXR_RELIABILITY_RELIABLE` | omitted from the wire; Agent's default (`create_entities_bin.c:148`) |

The Cyclone row is the one that matters: it is the backend that talks to real
ROS peers, it is the only one that disagrees with upstream's resolution, and it
folds the sentinel to the *less* safe of the two reliabilities. Liveliness is
the single policy handled correctly anywhere in the tree — `qos.cpp:62` skips
`dds_qset_liveliness` entirely when the value is
`NROS_RMW_LIVELINESS_SYSTEM_DEFAULT`, which is exactly the shape option A needs
for the other three.

Is that reachable today? Only from a caller that does not use our constants —
none of ours emit `0` for a policy — so a hand-rolled or zero-filled C
`rmw_qos_profile_t`, which the header explicitly contemplates for
`rx_buffer_hint` (`rmw_entity.h:643-647`). Latent, not currently exercised.

**We already spell the sentinel correctly one layer up.** The orchestration IR
has had it since phase-211: `QosReliability::SystemDefault` /
`QosDurability::SystemDefault` / `QosHistory::SystemDefault` /
`QosLiveliness::SystemDefault` at
`packages/cli/nros-cli-core/src/orchestration/schema.rs:58-88`, and the default
profile the planner emits for an unstated QoS is
`{"reliability": "system_default", …, "depth": 0, …}`
(`packages/cli/nros-cli-core/src/orchestration/planner.rs:1657-1671`) — i.e. the
IR already picked `0` as the depth sentinel, independently, and already cannot
lower itself into a `QoSProfile` without inventing values.

**Read-back exists and would make the resolution observable.** Issue 0823's
`read_entity_qos` (`qos.cpp:103-171`) reports what the entity actually holds,
and `report_qos_downgrade` (`packages/rmw/cffi/src/lib.rs:1868-1911`) warns on a
difference. Both already treat `granted.depth == 0` as "no answer" rather than
as an answer (`qos.cpp:138`, `lib.rs:1903`) — the sentinel reading, arrived at
twice by accident.

**One ordering constraint.** Under the zenoh path the QoS is serialised into the
liveliness-token keyexpr in upstream's numbering —
`QosKeyExpr::to_qos_string`, `packages/rmw/zenoh/nros-rmw-zenoh/src/keyexpr.rs:185-213`,
`"{reliability}:{durability}:{history},{depth}:…"` — which a ROS
`rmw_zenoh_cpp` peer parses out of the graph. Upstream resolves in
`best_available_qos` *before* the entity and its token exist, so its tokens never
carry a `0`. Ours must resolve at the same point, before anything derived from
QoS is computed, or we would advertise `0:0:0,0` to peers as if it were a
policy.

### 4. Recommendation — **A**: carry the sentinel, resolve it per backend at the create entry

The 1-vs-10 question has no right answer because the *shape* is wrong. Upstream
does not ship a profile; it ships an absence, and the two reference RMWs fill
that absence with 1 and 42. Whatever number we bake in, the name will lie to the
ported node that reads it — which is the only reader the constant has.

The change is smaller than it looks, because most of it exists:

1. **`depth == 0` is the depth sentinel**, matching upstream's
   `RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT = 0`. It is already free: Cyclone
   rejects 0 outright, XRCE already reads it as "unstated", and both
   `read_entity_qos` (`qos.cpp:138`) and `report_qos_downgrade`
   (`packages/rmw/cffi/src/lib.rs:1903`) already treat a 0 as "no answer".
2. **The C ABI is already there.** `NROS_RMW_{RELIABILITY,DURABILITY,HISTORY}_SYSTEM_DEFAULT`
   and `NROS_RMW_LIVELINESS_SYSTEM_DEFAULT` are all `0`
   (`rmw_entity.h:70,78,84,266`). `NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT` stops
   aliasing `_DEFAULT` (`rmw_entity.h:735`, mirror at `cffi/src/lib.rs:366`) and
   becomes the all-zero initialiser — which is also what a `memset` of the
   struct gives, so the ABI stops having two answers for the same bytes.
3. **`QoSProfile` grows the variant it lacks.** `SystemDefault` on
   `QoSHistoryPolicy` / `QoSReliabilityPolicy` / `QoSDurabilityPolicy`
   (`traits.rs:333-362`), non-`#[default]` so `QoSProfile::default()` keeps
   meaning `QOS_PROFILE_DEFAULT`.
4. **`required_policies` stops asserting what the caller did not ask for.**
   `traits.rs:1626-1655` starts from `CORE` unconditionally; a sentinel-valued
   policy must drop its bit, exactly as a zero `deadline_ms` already does.
   Without this, a `SYSTEM_DEFAULT` profile would demand `DEPTH` from a backend
   that has no per-entity depth and get `IncompatibleQos` for asking for
   nothing.
5. **Each backend resolves at its create entry, before anything is derived from
   the QoS**, and resolves to **what the corresponding upstream RMW does, not to
   the raw middleware default**. Interop with a ROS peer is the requirement, and
   the two differ: leaving `dds_qset_reliability` unset would give Cyclone's
   own reader default of `BEST_EFFORT` (`ddsi_plist.c:3470`), while
   `rmw_cyclonedds_cpp` deliberately picks `RELIABLE`. Ordering matters because
   the zenoh path serialises QoS into the liveliness-token keyexpr that ROS
   peers parse (`keyexpr.rs:185-214`) — upstream resolves in `best_available_qos`
   before the token exists, so its tokens never carry a `0`, and ours must not
   either.

   | backend | resolves `SYSTEM_DEFAULT` to |
   | --- | --- |
   | cyclonedds | RELIABLE / VOLATILE / KEEP_LAST / **depth 1** — mirroring `create_readwrite_qos`. This also repairs the `qos.cpp:25-30` fold, which today sends the sentinel to `BEST_EFFORT`. |
   | zenoh | RELIABLE / VOLATILE / KEEP_LAST / **depth `SUBSCRIBER_RING_DEPTH`** (4) — the number the shim actually enforces. Not 42: 42 is `rmw_zenoh_cpp`'s figure for its own buffers, and advertising a depth we cannot honour is the lie this whole issue is about. |
   | xrce | RELIABLE / VOLATILE / KEEP_LAST / **depth left at 0** — the client already encodes exactly this (`optional_history_depth = false`, `create_entities_bin.c:148`) and the Agent's DDS layer resolves it, which is the honest answer for a backend whose defaults are not ours. |

6. **The resolution becomes observable for free.** 0823 already built the
   read-back: `read_entity_qos` (`qos.cpp:103-171`) reports what the entity
   holds, so `*_get_actual_qos` on a `SYSTEM_DEFAULT` entity answers with the
   resolved profile — which is also what upstream does.

`qos_preset_parity`'s `system_default_divergence_is_pinned_until_0829` then
deletes and the constant folds into `qos_module_agrees_with_the_presets`, as
that test already instructs.

**If A cannot land in one phase, the interim is 1, not 10** — not as a
compromise but because it is the only defensible concrete number: it is what
`rmw_cyclonedds_cpp` resolves the sentinel to, and cyclonedds is our only
backend that meets a real ROS peer on the wire. `10` makes `SYSTEM_DEFAULT` a
byte-for-byte synonym of `DEFAULT`, which is a constant that cannot be wrong
because it does not say anything. Choosing 1 does not foreclose A; choosing 10
quietly argues the sentinel is unnecessary.

### 5. What it costs, and who changes behaviour

**Nobody in-tree changes behaviour.** No entity in `packages/`, `examples/` or
`book/` is created with any of the six spellings. The cost is entirely edit
surface plus the out-of-tree porting contract.

* **Three enums gain a variant, so every exhaustive `match` breaks.** The
  references are 63 / 45 / 35 for reliability / durability / history across
  `packages/`, most of them constructions rather than matches; the exhaustive
  matches to update are `packages/api/nros-c/src/qos.rs:156-172`,
  `packages/api/nros-cpp/src/lib.rs:362-378`,
  `packages/rmw/cffi/src/lib.rs:383-397`,
  `packages/rmw/zenoh/nros-rmw-zenoh/src/keyexpr.rs:189-201`, and
  `packages/api/nros/src/node_metadata.rs:1430-1444`. The compiler finds them
  all; none is subtle.
* **The user-facing C API needs a decision of its own.** `nros_qos_t` still uses
  the pre-phase-376 dense numbering, where `NROS_QOS_RELIABILITY_BEST_EFFORT = 0`
  (`packages/api/nros-c/include/nros/nros_generated.h:529-561`) — so a sentinel
  there cannot be `0` without an ABI break, and a zeroed `nros_qos_t` will keep
  meaning BEST_EFFORT while a zeroed `rmw_qos_profile_t` means SYSTEM_DEFAULT.
  Cheapest defensible answer: leave `nros-c` concrete and document the sentinel
  as an RMW-layer concept. That is a real remaining divergence, not a fix.
* `scripts/gen-abi-bindings.sh` + committed `generated.rs` regeneration if the
  header changes (`check-abi-bindings` gates it).
* **Two book pages become true instead of false.**
  `book/src/design/rmw-vs-upstream.md:439-443` and
  `book/src/concepts/ros2-comparison.md:167-170` both claim the standard profile
  constants including `_SYSTEM_DEFAULT` match upstream "field-for-field". For
  `_SYSTEM_DEFAULT` that is wrong on every field today.
* **Out-of-tree callers.** A ported node importing `SYSTEM_DEFAULT` today gets
  depth 1 or depth 10 depending on which of the two spellings it happened to
  reach. After A it gets 1 on cyclone, 4 on zenoh and the Agent's choice on
  XRCE — i.e. what ROS 2 promised it in the first place.

### 6. Two spin-offs found on the way — neither is 0829, both are real

* **The sentinel already resolves to opposite reliabilities in two backends.**
  `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/qos.cpp:25-30` folds
  `reliability == NROS_RMW_RELIABILITY_SYSTEM_DEFAULT` (0) to
  `DDS_RELIABILITY_BEST_EFFORT`, while
  `packages/rmw/xrce/nros-rmw-xrce/src/session.c:211-213` and
  `packages/rmw/cffi/src/rust_adapter.rs:258-263` fold the same value to
  RELIABLE. Reachable today from a zero-filled or hand-rolled C
  `rmw_qos_profile_t`; the Cyclone answer is both the odd one out and the unsafe
  one, and it is the backend that faces real ROS peers.
* **The zenoh backend's `CORE` mask over-promises.**
  `shim/session.rs:1245-1268` advertises RELIABILITY, DURABILITY_VOLATILE,
  HISTORY and DEPTH as honoured — "at the subscriber buffer level" for the last
  two — but all four are discovery metadata on that backend: read into the
  liveliness-token keyexpr and nowhere else. The ring is the build-time
  `SUBSCRIBER_RING_DEPTH` (`build.rs:37`, default 4). So `validate_against`
  accepts a profile the backend then ignores, which is the silent downgrade the
  mask exists to prevent. Worth its own issue; it is not 0829, but it does mean
  the zenoh row of the table above describes what the code already does rather
  than a new choice.

### 7. Not established

* **Line numbers for the three upstream files** (`rmw/qos_profiles.h`,
  `rmw/types.h`, `rmw_cyclonedds_cpp/src/rmw_node.cpp`,
  `rmw_zenoh_cpp/src/detail/qos.cpp`). No ROS is installed on this host; the
  code above is verbatim from the branch named, but the numbering the fetch
  reported was renumbered and is not trustworthy. Re-check on a host with ROS
  before quoting a line.
* **Whether `best_available_qos` is `rmw_zenoh_cpp`'s only resolution point**, and
  whether anything downstream re-reads the sentinel.
* **Whether a ROS `rmw_zenoh_cpp` peer's QoS matching considers the `depth`
  field of the liveliness-token keyexpr.** History/depth is not an RxO policy in
  DDS, which is why advertising our real 4 should be safe — but that is
  reasoning, not a measurement, and it decides whether the zenoh row of the
  resolution table above is right.
* **File:line for zenoh-pico 1.7.2's own constants.** `Z_PRIORITY_DATA` = 5,
  congestion control `DROP` for push, and `z_reliability_t::RELIABLE` = 0 were
  read out of the vendored 1.7.2 tree but are recorded here without a verified
  `file:line`. They do not bear on the recommendation — the shim, not
  zenoh-pico, resolves QoS on this path — but do not quote them as located.

* **Cyclone's behaviour if `make_dds_qos` simply omitted `dds_qset_*` for a
  sentinel field.** Read off the `ddsi_default_qos_reader` / `_writer` tables and
  the merge order, not measured. It is the reason the recommendation resolves
  explicitly rather than by omission, so it is worth confirming before
  implementing.
