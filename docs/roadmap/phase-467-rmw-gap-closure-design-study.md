# Phase 467 — the last thirteen `gap` rows: a design study, not an implementation

**Status (2026-09-25). STUDY ONLY, and deliberately so. Nothing here is
implemented and no build was run — ten of the thirteen rows below carry a
"needs a compile nobody ran" line, which is a finding, not an apology. The
deliverable is a decision per row: four of the thirteen are genuine design
questions and are stated as questions with options and a recommendation; the
other nine are sized. Read §"Sequencing" first — it is the only section that
answers "what do I do Monday".**

Implements RFC-0089 (the compile-or-conform rule and the four dispositions) and
RFC-0036 (a divergence must name a platform constraint, never a preference).
Continues [phase-417](phase-417-ros2-api-adoption.md) §"Disposition before
implementation" and [phase-444](phase-444-rmw-fix-up.md) §"The ROS 2 gap list".
Neither is superseded; this phase owns the thirteen rows and nothing else.

## What this measures, and where the tree disagreed with the brief

Re-derived on **2026-09-25** against the tree this branch is based on, straight
off `docs/reference/api-parity-ledger/*.json` the way phase-417 insists
(`verdict == "gap"`, `_`-prefixed keys skipped):

```
init 2, lifecycle 1, log 3, node 1, other 1, pubsub 4, timer 1  —  13 total
```

**It was TWELVE when this study was measured and it is THIRTEEN now, and that
is a finding rather than an erratum.** The measurement was taken on 2026-09-24
and `main` moved 88 commits before this document was committed.
`cpp:Subscription::get_actual_qos` arrived in that window: phase-456 W2b split
one subscription class into two, the dispatch half lost an accessor, and the
split RECORDED the loss rather than inventing a signature for it — which is the
behaviour the campaign wants, and the reason the queue grew. It is Row 13.

Two consequences worth stating rather than quietly absorbing:

* **This queue is not a fixed backlog draining to zero.** It has an inflow, and
  the inflow is other people's correct decisions. "Thirteen left" is a
  snapshot; re-derive before quoting it, which is the rule phase-417 already
  wrote for itself after its own count went stale twice.
* **Every file:line citation here was re-verified on 2026-09-25 and nineteen
  had drifted** in those 88 commits, most of them in
  `packages/api/nros-cpp/include/nros/node.hpp` and
  `packages/cli/nros-cli-core/src/entity_inventory.rs`. This document has a
  section about citation drift in the ledger; drift turns out to be a property
  of writing anything down in this tree, not of the ledger.

For the record, the trajectory: 80 `gap` rows on 2026-09-21, 12 on 2026-09-24,
13 on 2026-09-25. Every shard phase-417 called flat — pubsub 29, graph 18,
service 14 — is now 4, 0 and 0.

Four things the tree said back that the framing of this study started from did
not:

1. **The `/rosout` question spans three rows and only ONE of them is a `gap`.**
   `c:logging_rosout_enabled` is `gap` + `absent`; `cpp:RosoutQoS` is
   **`declined`** + `adopt` and `cpp:NodeOptions::rosout_qos` is
   **`declined`** + `absent`. So this is not a disagreement among gap rows — it
   is a `gap` row and two `declined` rows making opposite claims about whether
   anybody decided. That is a sharper problem, not a softer one: `declined`
   means "we deliberately do not", and one of the three is wrong.
2. **`cpp:Publisher::assert_liveliness` is not uniformly a no-op.** Cyclone
   implements the slot — `dds_assert_liveliness` on the writer, gated on the
   liveliness kind (`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/publisher.cpp:314`,
   registered at `vtable.cpp:372`), landed for issue 1231. The row's "no wire
   traffic" finding is true of **zenoh**, and the zenoh shim is not inert
   either: its stored timestamp drives a LOCAL `LivelinessLost` event
   (`shim/publisher.rs:761-800`). What is missing on zenoh is anything a REMOTE
   peer can observe.
3. **Zenoh already runs a standing liveliness SUBSCRIBER, not a poll.**
   `zpico_graph_cache_start` declares `z_liveliness_declare_subscriber` with
   `history = true` (`packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c:4031`),
   reached from `ZenohSession::ensure_graph_cache`
   (`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs:828`). The
   graph-change signal already ARRIVES; nothing forwards it. Cyclone is the
   opposite — every graph reader is a polled `dds_read` created with a
   `nullptr` listener (`src/graph.cpp:359`, `src/graph_query.cpp:95`), so
   Cyclone has no change signal at all. This asymmetry is the whole of row 8's
   cost and the ledger does not record it.
4. **`rust:Time::to_ros_msg` has a hard blocker with an issue already open**,
   and it is not the one the row names. Issue 1428
   ([docs/issues/1428-builtin-interfaces-generated-three-times-none-canonical.md](../issues/1428-builtin-interfaces-generated-three-times-none-canonical.md))
   measured that `builtin_interfaces` is pre-generated **three times** under
   `packages/interfaces/` — `nros-builtin-interfaces`,
   `nros-builtin-interfaces-diag`, `nros-builtin-interfaces-clock`, byte-
   identical sources, three manifest vintages — and none is canonical, so there
   is no crate name a consumer can name. The row asks which siting; the tree
   asks which crate first.

### Citation drift found while measuring

The 2026-09-23 re-read corrected most cites. These three are still wrong, and
each is a one-line ledger fix rather than work:

| row | says | is |
| --- | --- | --- |
| `cpp:RosoutQoS` | `qos.hpp:745` | `qos.hpp:977` (`:744` is `qos_all_unknown()`) |
| `cpp:Node::create_subscription` | `node.hpp:1458`, guard `:1455-1457` | `node.hpp:1551`, guard `:1548-1550` |
| `rust:Time::to_ros_msg` | `cargo-nano-ros/src/lib.rs:720` | `:790` (`:714` is `bundled_interfaces_dir()`) |

Also: `NROS_MAX_PENDING_REQUESTS`, the constant with zero occurrences outside
the ledger, is on a **service** row, and `service.json` has no `gap` rows left.
It is not one of the thirteen.

## Sequencing — which are independent, which share a blocker, cheapest order

**Eleven of the thirteen are independent of each other.** The two that are not are
the pair worth the most: `c:log_severity_t` and `rust:Logger::set_default_level`
are ONE mechanism seen from two languages, and building either one first
correctly builds the other.

### The one real shared blocker: an "unset" logger level

- `c:log_severity_t` needs `UNSET` to mean INHERIT, which upstream resolves by
  walking the dotted ancestry to the default (read in the `ros2` box:
  `/opt/ros/humble/include/rcutils/rcutils/logging.h:425-452`; see Row 5 for
  what that header does and does NOT promise).
- `rust:Logger::set_default_level` needs a process-wide default that a newly
  interned logger starts at.

`nros_log` has neither, and **it has no dotted hierarchy at all**:
`InternTable::lookup` is a linear scan over 32 slots comparing
`logger.name() == name` exactly (`packages/core/nros-log/src/lib.rs:324-339`,
`MAX_LOGGERS = 32` at `:298`). With no ancestry, rcutils's walk **degenerates
to exactly one step: the process default**. So the same `AtomicU8` default plus
one "unset" sentinel in `Logger::level` closes both rows, and the bound
`c:log_severity_t` already carries as `adopt-bounded` ("no ancestry") is the
honest statement of what is not built.

Do these two together or neither. Doing `set_default_level` alone and
`log_severity_t` later means two passes over the same three functions, and
doing `log_severity_t` alone leaves a default nothing can set.

### Everything else

| row | depends on | blocks |
| --- | --- | --- |
| `rust:Context::domain_id` | nothing | nothing |
| `cpp:Node::create_subscription` | nothing | nothing |
| `rust:init_with_args` | the remap-overlay wave (not scheduled) | nothing |
| `rust:Session::serialization_format` | nothing (a trait edit) | nothing |
| `cpp:Publisher::get_gid` | Q1's identity decision | nothing |
| `cpp:Publisher::assert_liveliness` | a zenoh wire decision | nothing |
| `c:node_get_graph_guard_condition` | a backend edge, per backend | nothing |
| `c:lifecycle_change_state` | nothing; it is just expensive | nothing |
| `c:logging_rosout_enabled` | Q4's verdict | the two `declined` rows |
| `rust:Time::to_ros_msg` | **issue 1428** | nothing |
| `cpp:Subscription::get_actual_qos` | an executor reachable from the dispatch object | nothing |
| `c:log_severity_t` | the unset-level mechanism | — |
| `rust:Logger::set_default_level` | the unset-level mechanism | — |

### Cheapest order

1. **`rust:Context::domain_id`** — one accessor, once Row 6's return type is
   picked. Minutes, plus the compile.
2. **The ledger corrections above**, and whichever of Q4's three rows is wrong.
   Zero code.
3. **`rust:Session::serialization_format`** — a small, contained trait edit with
   exactly one in-tree caller of the method. It is a whole-workspace recompile
   and nothing else. Do it while the tree is otherwise quiet.
4. **`cpp:Node::create_subscription`** — two in-tree call sites of the affected
   overload. Cheap once the preference is settled; see Row 12.
5. **The logger-level pair** (`c:log_severity_t` + `rust:Logger::set_default_level`)
   — one mechanism, two rows, both closed.
6. **`cpp:Publisher::get_gid`** — cheap ONCE Q1 is answered; the accessor itself
   is a forwarder with an `UNSUPPORTED` arm.
7. **`c:node_get_graph_guard_condition`, zenoh half only.** The signal is
   already delivered to a C callback; forwarding it is small. Cyclone's half is
   a separate, larger item and should not hold the row.
8. **`c:lifecycle_change_state`** — self-contained but genuinely expensive
   (a new session entity in every lifecycle image). Schedule it, do not squeeze
   it in.
9. **`cpp:Publisher::assert_liveliness`** — needs a zenoh wire decision that
   nobody has asked for; see Row 7.
10. **`cpp:Subscription::get_actual_qos`** — the newest row, and the one whose
    answer belongs to phase-456 rather than here; see Row 13.
11. **`rust:Time::to_ros_msg`** — blocked behind issue 1428. Do not start.
12. **`rust:init_with_args`** — owed to a wave that does not exist yet. Leave.

Steps 1–5 are a day's work plus the tier each earns. Steps 6–12 are each their
own item.

---

# The four decisions

## Q1 — the publisher GID: what is the right reconciliation, and what does it cost?

### What is actually true

The row says the two gids "are the SAME identifier under upstream semantics"
and disagree in width, 24 versus 16. Measured, **the width is the smaller
half of the problem**:

| identifier | width | who PRODUCES it | who reads it |
| --- | --- | --- | --- |
| `rmw_gid_t::data` (our ABI, `rmw_entity.h:104`, `:212`) | 24 B + an `implementation_identifier` pointer | **Cyclone only** — the 16-byte DDS writer GUID, zero-padded to 24 (`vtable.cpp:146-164`) | tests only |
| `MessageInfo::publisher_gid` (`nros-core/src/message_info.rs:17`) | 16 B | **zenoh only** — `RmwAttachment::generate_gid()`, a counter × constant XOR a stack address (`shim/mod.rs:362-376`) | one test bin, which prints it |
| `nros_endpoint_info_t::endpoint_gid` (C graph surface) | 24 B | Cyclone graph | the graph API |

Upstream Humble's `RMW_GID_STORAGE_SIZE` is **24** — measured in the `ros2`
box, `/opt/ros/humble/include/rmw/rmw/types.h:42`. Our ABI is already correct.

So the honest statement is not "one identifier, two widths". It is:

> **No backend produces both.** On Cyclone, `get_gid_for_publisher` returns a
> real DDS GUID and `MessageInfo::publisher_gid` is all zeros, because nothing
> fills it. On zenoh, `MessageInfo::publisher_gid` is a per-publisher random
> value and `get_gid_for_publisher` is `None`. There is no image in which the
> two can be compared, so today's silence is not yet a wrong answer — it is
> the absence of any answer.

Three further measurements that constrain the options:

* **The 16 is a WIRE constant and cannot move.** `RMW_ATTACHMENT_SIZE = 8 + 8 +
  1 + RMW_GID_SIZE` (`shim/mod.rs:93-97`); the attachment writes a VLE length
  byte of 16 and the reader REJECTS a length that is not 16 (`mod.rs:430`).
  That layout is `rmw_zenoh_cpp`'s, not ours. Widening it breaks interop with
  stock `rmw_zenoh`.
* **Nothing else depends on 16.** `PUBLISHER_GID_SIZE` has ten occurrences, all
  in `message_info.rs` and two re-exports. `MessageInfo` is not `repr(C)` and
  crosses no ABI. There is no `static_assert` tied to 16 anywhere; every gid
  assertion in the tree asserts 24.
* **`MessageInfo::publisher_gid` has no runtime reader.** The only comparison
  is the derived `PartialEq`, exercised by one default-value unit test.

### The options

**(a) Widen `MessageInfo::publisher_gid` to 24, zero-padded at the API
boundary.** The zenoh attachment stays 16 bytes on the wire; the shim
zero-extends into the 24-byte field exactly as Cyclone already zero-extends its
16-byte GUID. Then `MessageInfo::publisher_gid` and `rmw_gid_t::data` are one
type, comparable by construction, and `Publisher::get_gid()` ships in all three
languages with an `UNSUPPORTED` arm where the slot is NULL.
*Cost:* one const, one field type, one zenoh assignment, one C++ `kEndpointGidSize`-
shaped constant if the field ever reaches C (it does not today). No wire change,
no ABI header change, no gate.
*Risk:* the two identifiers become type-compatible while remaining semantically
unrelated on every backend — zenoh's is a PRNG value, Cyclone's a GUID — so a
user CAN now compare two things that will never be equal. That is the
RFC-0089 Part I shape moved one step, not removed.

**(b) Widen, AND make each backend produce both from one source.** Zenoh's
publisher already has a stable identity in its liveliness token — `ZenohId`
(16 B) plus a `u32` entity id (`zpico.rs:147`, `shim/mod.rs:594`). Deriving the
attachment gid from that pair instead of from a stack-address PRNG makes the
zenoh gid mean something, and filling `get_gid_for_publisher` from the same
bytes makes the two answers agree. Cyclone fills `MessageInfo::publisher_gid`
from the sample's publication handle, which it already has.
*Cost:* (a), plus a zenoh gid-derivation change (which is observable on the
wire to an `rmw_zenoh_cpp` peer — it reads our attachment) and a Cyclone
receive-path change. Needs an interop run against a live peer.
*Risk:* the gid derivation is the one thing in this list a stock ROS 2 peer
looks at. It cannot be measured without the router-and-peer lane phase-444 W2
still owes.

**(c) Do not unify; rename ours and ship `get_gid()` only where the slot is
filled.** `MessageInfo::publisher_gid` becomes `publisher_tag` or similar, the
doc says it is a per-sample sender tag and NOT an `rmw_gid_t`, and
`Publisher::get_gid()` answers `UNSUPPORTED` on zenoh/uORB/XRCE.
*Cost:* a rename on a field with no runtime reader, plus the accessor.
*Risk:* it walks away from upstream's semantics (they ARE the same identifier
upstream), and it creates a name a ported rclrs file does not have.

### Recommendation

**(a) now, (b) filed as its own item, never (c).**

The reasoning is that (a) removes the type-level lie for the price of one
constant and blocks nothing, while (b) is a wire-observable change that cannot
be accepted without the live-peer lane. Splitting them keeps
`cpp:Publisher::get_gid` closable this week. Say the bound out loud on the
accessor and in the ledger: *a gid obtained from a take and a gid obtained from
`get_gid()` are the same TYPE and, on every backend we ship today, are not
produced from the same source — comparing them is meaningful only once (b)
lands.* That sentence is what keeps (a) from being the silent-difference shape
RFC-0089 Part I refuses.

**Flag:** (a) touches no committed ABI header and needs no regeneration. (b)
does not either — `rmw_gid_t` already has the right shape — but it changes an
interop wire value.

**Needs a compile nobody ran:** yes, for both. Widening a field in `nros-core`
recompiles most of the workspace.

## Q2 — `serialization_format`: what shape lets a backend say "I have not said"?

### What is actually true

Confirmed, and worse than the row states in one respect and better in another.

* The contract is ours: `rmw_vtable.h:929-932`, the NULL-slot paragraph, "the
  runtime answers NULL — it does NOT guess `cdr`".
* The inherent path obeys it: `CffiSession::serialization_format_cstr`
  (`packages/rmw/cffi/src/lib.rs:1959`) returns NULL, and
  `CffiSession::serialization_format` (`:1970`) returns `Option<&'static str>`.
* The trait impl breaks it: `packages/rmw/cffi/src/lib.rs:2619` is
  `CffiSession::serialization_format(self).unwrap_or(Self::SERIALIZATION_FORMAT)`,
  and the trait const is the literal `"cdr"` (`nros-rmw/src/traits.rs:1618`).
  The doc comment at `:2614-2618` rationalises it, which is the part the row
  correctly calls worse than the bare `unwrap_or`.
* **Better than feared:** the C and C++ entry points do NOT go through the
  trait. `nros_node_get_serialization_format` (`nros-c/src/node.rs:775`) and
  `nros_cpp_node_get_serialization_format` (`nros-cpp/src/lib.rs:2521`) both
  call `serialization_format_cstr` directly and propagate NULL. **Only the Rust
  surface is wrong.**
* **The blast radius of the trait edit is one caller.** The only in-tree caller
  of `Session::serialization_format` is
  `packages/core/nros-node/src/executor/node.rs:164`. Seven types implement
  `Session`; six ride the default and one (`CffiSession`) overrides.
* `SerializationFormatId` (`nros-serdes/src/format.rs:34`) is `Cdr = 1`,
  `Uorb = 2`. **There is no "not stated" variant**, and discriminant 0 is
  unused but unnamed.

### The options

**(a) `fn serialization_format(&self) -> Option<&'static str>`, default
`Some(Self::SERIALIZATION_FORMAT)`.** `CffiSession` overrides by returning the
inherent `Option` unchanged — the `unwrap_or` and its doc comment are deleted.
`node.rs:164` propagates the `Option`. The six defaulting impls are untouched:
a backend whose format IS a compile-time fact still says `Some`, which is true.
*Cost:* one trait method, one override, one forwarder; a whole-workspace
recompile because `nros-rmw` is under 18 direct dependents and, transitively,
nearly everything. No gate checks the Rust trait against the C slot, so nothing
regenerates.
*Risk:* `Session` is re-exported from the facade
(`packages/api/nros/src/lib.rs:1041`), so this is a user-visible Rust signature
change. It is exactly the change RFC-0089 permits — the compiler points at
every call site.

**(b) Keep `&'static str` and make the trait method FAIL rather than guess.**
Returns `Result<&'static str, _>` or panics. Rejected on sight: a panic in an
accessor on a no-panic-path crate is worse than the guess, and a `Result` here
carries no more information than an `Option` while costing an error type.

**(c) Add an `Unknown` / `Unstated` variant to `SerializationFormatId` and
return the enum.** Tempting because it makes the absence nameable, but the
module doc is explicit that the u8 is **image-local and never crosses images**
while the STRING is the cross-image identity, and two C/C++ const-assert pairs
(`nros-c/src/constants.rs:127-141`,
`nros-cpp/src/lib.rs:5749-5764`) pin the enum against generated macros. Adding
a variant there pays for a second mechanism to say what `Option::None` already
says.

**(d) Leave it, and fix only the doc comment.** Honest about cost — zero — and
it does not close the row: `Self::SERIALIZATION_FORMAT` is still four bytes
spelling `cdr` at every Rust call site.

### Recommendation

**(a).** It is the change the header already specifies; the row is right that
this is "a fix and not a decision". The only real question was blast radius,
and the measurement answers it: one caller, one override, six untouched impls.
Land it on a quiet tree because it recompiles the workspace, and delete the
rationalising doc comment in the same commit — a comment asserting a
distinction the type does not carry is what kept this open.

**Flag: touches `packages/core/nros-rmw` traits. Every backend recompiles.**
No ABI header, no vtable, no bindgen regeneration, no `NROS_CODEGEN_VERSION`.

**Needs a compile nobody ran:** yes. That is the whole cost.

## Q3 — `rust:Time::to_ros_msg`: which siting, and does the `Result` survive?

### What is actually true

* `nros_core::time::Time` (`packages/core/nros-core/src/time.rs:37`) IS
  `builtin_interfaces/msg/Time`'s field set — `sec: i32`, `nanosec: u32` — with
  no `From`/`Into` impl anywhere in the file.
* The C++ trick works because C++ has structural typing:
  `template <typename TimeMsgT> void to_msg(TimeMsgT& out)`
  (`packages/api/nros-cpp/include/nros/time.hpp:129`, line verified).
* rclrs's signature is
  `to_ros_msg(&self) -> Result<builtin_interfaces::msg::Time, TryFromIntError>`
  (`docs/reference/api-surface/rclrs.json`).
* **`nros-core` cannot host the method as an inherent one**, ever. Its deps are
  `log`, `nros-serdes`, `heapless` and nothing else, and every generated message
  crate depends on `nros-core` — so naming a message type there is a cycle.
  Issue 1428 records this independently.
* **The blocker is issue 1428, not the siting.** `builtin_interfaces` is
  pre-generated three times under `packages/interfaces/`, byte-identical, with
  three manifest vintages; giving two of them the `links` key the third has
  makes the whole workspace unresolvable (measured and reverted in the issue).
  There is no crate name to depend on.

### The options (once 1428 is resolved)

**(a) A trait in `nros-core` that codegen implements** — the Rust analogue of
the C++ template. `trait FromRosTime { fn from_ros_time(t: Time) -> Self; }` or
the direction the row sketches.
*Cost:* **an `NROS_CODEGEN_VERSION` bump.** The bump policy names exactly this
trigger — "a trait signature generated code implements". That is a four-file
change (`packages/core/nros-core/src/codegen_version.rs`,
`.config/codegen-version-surface.txt`,
`packages/api/nros-c/include/nros/nros_config_generated_nuttx.h` in two arms,
`packages/api/nros-cpp/include/nros/nros_cpp_config_generated_nuttx.h`), plus
re-blessing 26 golden fixtures under
`packages/cli/rosidl-codegen/tests/fixtures/fingerprint-corpus/`, plus a
template edit in `packs/nros/message.rs.jinja`, plus every generated tree in
every checkout invalidated. Two gates hold it:
`check-codegen-version-surface` and `check-config-fallback-macros`.
*Benefit:* it works for ANY message with `sec`/`nanosec`, which is what the C++
template buys, and it needs no canonical `builtin_interfaces` crate — so it
is the one option 1428 does NOT block.

**(b) `impl From<nros_core::Time> for Time` sited in the generated
`builtin_interfaces` crate.** No trait, no codegen-version bump (an added impl
on a generated type is a template edit, and the policy says do not bump for a
cosmetic one — though "does a new impl change the interface generated code
implements" is a judgment the bump policy's own wording makes arguable, and it
should be asked before, not after).
*Cost:* a conditional block in the template keyed on the package/message name,
which is the shape the `nros` pack has avoided so far (it emits zero `From`s
today).
*Blocked by 1428:* yes — it produces three impls in three crates for one wire
type.

**(c) Site it in the facade** (`packages/api/nros/`), which may name a message
crate. Blocked by 1428 for the same reason, and it puts a conversion for a
core type behind the `alloc`-ish facade that freestanding images do not all
take.

### Does the `Result` survive?

**No, and it should not.** rclrs needs `Result<_, TryFromIntError>` because its
`Time` is `i64` nanoseconds and narrowing to `sec: i32` can overflow. Ours is
ALREADY `{ sec: i32, nanosec: u32 }`, so the conversion is two field copies and
cannot fail. Shipping an infallible operation behind a `Result` to make
`t.to_ros_msg()?` compile is the inverse of RFC-0089's rule: it hides a
difference instead of making the compiler point at it, and it teaches every
call site to handle an error that does not exist.

Recommendation on the signature: **infallible, and let the port be a mechanical
edit.** A ported `t.to_ros_msg()?` fails to compile with "the `?` operator can
only be applied to values that implement `Try`" — loud, local, one character to
fix. If that reads as too sharp, the bounded alternative is to keep the name
infallible and record `adopt-bounded` with the `?` in the reason; do not add a
`Result`.

### Recommendation

**Do not start this row. Resolve issue 1428 first**, then prefer **(a)**, the
trait, because it is the only option that is not downstream of picking a
canonical crate, and because it is the shape the C++ lane already proved. Enter
it knowing it costs a codegen-version bump — that is the price of the
structural-typing substitute, and it is worth stating in the row so nobody
prices this as "one `impl From`".

## Q4 — settle the `/rosout` family one way

### What is actually true

Three rows, two verdicts, one fact:

| row | verdict | disposition | what it says |
| --- | --- | --- | --- |
| `c:logging_rosout_enabled` (log.json) | `gap` | `absent` | "If `/rosout` is ever built, this row becomes `adopt` on the same day" |
| `cpp:RosoutQoS` (qos.json) | **`declined`** | `adopt` (its prose still says `ADOPT-BOUNDED`) | "THE DECLINE RECORDED HERE IS ABOUT THE TOPIC AND IT STANDS" |
| `cpp:NodeOptions::rosout_qos` (qos.json) | **`declined`** | `absent` | not declared at all; a ported call gets `no member named` |

And the code, measured:

* No `RosoutSink` exists. `git grep RosoutSink -- packages` finds the
  README's "a future `RosoutSink`" and nothing else.
* The book says "explicitly out of scope **today**"
  (`book/src/user-guide/logging.md:213`).
* `rcl_interfaces/msg/Log` **already ships as a pre-generated crate**
  (`packages/interfaces/rcl-interfaces/generated/humble/nros-rcl-interfaces/src/msg/log.rs`),
  with four `heapless::String<256>` fields — about 1 KB by value.
* `nros-log` **cannot host the sink**: it is `#![no_std]` with dependencies
  `heapless`, `log` (optional) and `portable-atomic`, and no path to a node or
  a publisher. The designed hook exists and is the right one —
  `add_sink` / `MAX_ADDED_SINKS = 4` (`packages/core/nros-log/src/lib.rs:625`,
  `:588`), documented as "for consumers that TEE (a /rosout bridge, a test
  collector)".
* `enable_rosout` is `refuse-loud` (`packages/api/nros-cpp/include/nros/options.hpp:256`),
  and its rationale block says plainly: "nano-ros has no runtime
  ComponentManager, no intra-process transport, no topic-statistics collector
  and no `/rosout` topic … There is nothing behind these knobs to switch."

### The options

**(a) Make all three `gap`.** The book's word is "today", the sink list was
SHAPED for this sink, the message crate exists, and the tee hook exists.
*Cost of saying it:* zero. *Cost implied:* a `RosoutSink` is a publisher in
every image that enables it — `EntityInventory::derive`'s publisher and
liveliness counts move, and a `Log` sample is ~1 KB, which on a 16 KB picolibc
arena is not free. Saying `gap` commits nobody to building it; that is what
`gap` means.

**(b) Make all three `declined`.** `/rosout` is a host-ROS operator affordance
— `rqt_console`, `ros2 topic echo /rosout`, launch-side aggregation — and an
RTOS image logs to a board console. The C++ refusal already states the reason
as a settled one, and RFC-0036 permits a decline with a stated reason.
*Cost of saying it:* zero. *What it costs a user:* exactly what the rows
already say — no operator-side log stream.

**(c) Split by concept: the QoS PROFILE is `adopt`/shipped, the TOPIC is the
question.** This is very nearly what `cpp:RosoutQoS` already says, and it is
why that row is confusing: it carries `declined` (about the topic) and a
profile-side disposition (about the profile) in one row, so it reads as two
claims — and as of 2026-09-25 the two halves have come apart in the FILE: the
`disposition` field reads `adopt` while the prose still says "ADOPT-BOUNDED on
exactly that point". Nothing gates a disposition against its own reason, so
that disagreement is silent.

### Recommendation

**(a): make the family `gap`, with `c:logging_rosout_enabled` keeping `absent`
and the two qos.json rows re-verdicted.** Three reasons, in priority order:

1. **`declined` is a stronger claim than anyone made.** Under the schema,
   `declined` means "ROS 2 has it, we deliberately do not, with the reason".
   Nobody decided this — the book says "today", the sink list was shaped for
   it, and the log message type is already generated and committed. A `gap` is
   "nobody has done it", which is the measured state.
2. **The two verdicts are not equally reversible.** A `gap` that turns out to
   be a decline costs one ledger edit. A `declined` that is really a gap
   removes the item from the campaign's work list permanently — the `gap` rows
   ARE the queue (phase-417 says so in a heading) — and nothing re-asks.
3. **The constraint the refusal names is real but narrower than the topic.**
   `enable_rosout` refuses because there is no `/rosout` topic, not because a
   `/rosout` topic is impossible. Those are different sentences, and only the
   second would justify `declined`.

Keep `cpp:NodeOptions::rosout_qos` at `absent` as its DISPOSITION under the new
`gap` verdict — it is reached only through a refused type, which is RFC-0089's
own rule and is correct regardless of the verdict. And **split `cpp:RosoutQoS`'s
two claims into two sentences** so the profile half ("shipped, correlates
`same`") stops reading as a caveat on the topic half.

Concretely, this is a `qos.json` edit plus one sentence in `log.json`, and no
code. It is the cheapest item in this entire study and it is the one that stops
three rows lying to each other.

---

# The other eight rows

## Row 5 — `c:log_severity_t` [gap, adopt-bounded]: is the envelope permanent?

**Owed:** `UNSET` means INHERIT upstream; here it is the numeric floor and
selects `TRACE`, the most verbose level. Our side: `to_facade`
(`packages/api/nros-c/src/log.rs:94-103`) bands, and `_ => Severity::Trace`
catches 0. The envelope is now STATED in both the header
(`packages/api/nros-c/include/nros/log.h:58-68`) and the doc comment, which the
2026-09-23 pass fixed.

### Read the Humble header, and read it IN THE BOX

Worth recording as method, because it went wrong once inside this study: on the
HOST `/opt/ros` does not exist, and a search that runs there finds no rcutils
at all and may fall back on a Foxy copy vendored in some unrelated tree. Inside
the `ros2` distrobox the file is at exactly the path the ledger cites. **The
box rule applies to READING, not only to building.** A Foxy quotation is not
evidence for a Humble claim.

Read there (`/opt/ros/humble/include/rcutils/rcutils/logging.h`), verbatim:

* `:43` — `#define RCUTILS_DEFAULT_LOGGER_DEFAULT_LEVEL RCUTILS_LOG_SEVERITY_INFO`.
* `:425-434`, `get_logger_effective_level` — "the severity level of the logger
  if it is set, otherwise … the first specified severity level of the logger's
  ancestors … signified by logger names being separated by dots … If the level
  has not been set for the logger nor any of its ancestors, the default level
  is used."
* `:312-316`, `set_default_logger_level` — "If the severity level requested is
  `RCUTILS_LOG_SEVERITY_UNSET`, the default value for the default logger
  (`RCUTILS_DEFAULT_LOGGER_DEFAULT_LEVEL`) will be restored instead."
* `:399-402`, `get_logger_leveln` — returns "`RCUTILS_LOG_SEVERITY_UNSET` if
  unset".

### Our own header overreaches, and this is a real correction

`packages/api/nros-c/include/nros/log.h:61-63` and
`packages/api/nros-c/src/log.rs:85-87` both assert that
`rcutils_logging_set_logger_level(name, RCUTILS_LOG_SEVERITY_UNSET)` **UNSETS**
that logger's level. **The `set_logger_level` doc block does not say that.**
Read at `:382-403`, the whole of its behavioural text is: *"If an empty string
is specified as the name, the `g_rcutils_logging_default_logger_level` will be
set."* Nothing about `UNSET` as a VALUE. The "UNSET restores the default"
sentence belongs to `set_default_logger_level`, a different function, and the
"returns UNSET if unset" sentence belongs to `get_logger_leveln`.

So our claim is an INFERENCE stitched from three places — the enum's `UNSET =
0`, `get_logger_leveln`'s "UNSET if unset", and the effective-level ancestry
walk. It may well be right. It is not what the function we name promises, and
we are telling users it is.

**Whether storing `UNSET` through `set_logger_level` is honoured as an unset,
rejected as an invalid argument, or stored as a literal 0 that the severity map
treats specially is decided in `logging.c`, which nobody has read.** Recorded
as unresolved rather than guessed: `logging.c` is **not on this machine** —
`/opt/ros/humble` ships headers and libraries only (`find /opt/ros -name
logging.c` is empty; `/opt/ros/humble/src` holds only `gmock_vendor` and
`gtest_vendor`), and the box has no `deb-src` line, so `apt-get source
ros-humble-rcutils` cannot fetch it without editing apt sources, which needs
`sudo`. Resolving it means reading `rcutils/src/logging.c` at the `humble`
branch from the upstream repository.

**Two things this does NOT change.** The numbering is untouched (`UNSET=0,
DEBUG=10, INFO=20, WARN=30, ERROR=40, FATAL=50`; upstream has no TRACE, so our
`5` sits in the `UNSET`..`DEBUG` gap and collides with nothing), and the
divergence the row records is untouched: upstream resolves an unset level by
walking a dotted ancestry to a default and `nros_log` has no ancestry at all.
Row 5's substance stands; its supporting citation in our own header does not,
and the header should be narrowed to what the header it cites actually says
until `logging.c` is read.

**The mechanism:** see §Sequencing. One process-wide `AtomicU8` default plus an
"unset" sentinel in `Logger::level`. Note the sentinel cannot be 0 —
`Severity::Trace = 0` (`nros-log/src/lib.rs:83`) — so it has to be a value
outside the enum (`u8::MAX` reads naturally), and `is_enabled`
(`:246-247`) currently compares the raw byte, so it gains a branch.

**Cost, and it is a real one:** `is_enabled` is the hot path of every log macro
on every target. Adding "if unset, read the process default" puts a second
atomic load on it. The cheap form is to resolve the default at INTERN time
(write the default into the logger's atomic when it is registered) so the hot
path is unchanged — but that only works for loggers that go through
`register_logger` / `get_or_create_logger` / `resolve_logger`, NOT for a
`static L: Logger = Logger::new("x")` whose level is baked by a `const fn`. And
`Logger::new` hard-codes `Info` while `with_level` sets a level explicitly, so
at registration the two are **indistinguishable** — applying a default there
would silently override an author's `with_level`. That is why the sentinel is
needed and why this is a capability, not a rename.

**Recommendation:** build it, together with row 10, and keep `adopt-bounded`
with the bound restated as "a process default, no dotted ancestry" — which, in
a tree whose logger names are flat and whose intern table is a 32-slot exact-match
scan, is behaviourally complete. Do NOT build dotted ancestry: there is no
hierarchical name in the tree to walk, and a walk over a linear 32-slot table
is a cost with no consumer.

**Risk:** low, bounded, and entirely inside `nros-log` plus one `to_facade` arm.
**Needs a compile nobody ran:** yes, and a `just ci gate` for the hot-path change.

## Row 6 — `rust:Context::domain_id` [gap, absent]: the return type IS the question

**Owed:** an accessor. `Context::domain_id` is a public `u32` field
(`packages/api/nros/src/init.rs:183`, verified), rclrs's is
`domain_id(&self) -> usize`, and `InitOptions::with_domain_id` /
`set_domain_id` / `domain_id` already speak `usize` / `Option<usize>` — with a
doc comment that says why: "`usize` because that is rclrs's signature."

**Options:** `-> usize` (matches rclrs and our own `InitOptions`); `-> u32`
(matches the field, disagrees with both). Fields and methods are in different
namespaces in Rust, so the accessor and the `pub domain_id` field coexist.

**Recommendation: `pub fn domain_id(&self) -> usize { self.domain_id as usize }`.**
`InitOptions` already made this call and wrote down the reasoning; making
`Context` disagree with its own options type would be a second, quieter
inconsistency for no gain. The `as usize` widening is lossless on every target
we build (`usize >= 32 bits` everywhere in the tree).

**Also re-verdict:** the disposition is `absent`, which under RFC-0089 means "no
ported file can reach it". A ported `ctx.domain_id()` reaches it and fails to
compile — that is `adopt`, not `absent`, and the row's own text says the closure
is one line. Fix the disposition in the same edit.

**Risk:** minimal. **Needs a compile nobody ran:** yes, trivially.

## Row 7 — `cpp:Publisher::assert_liveliness` [gap, adopt]: what would real assertion require?

**Owed:** the row's return-type half is closed in practice (`Result` is
`[[nodiscard]]`-shaped and inverts loudly). What is open is behaviour, and it
is per backend:

| backend | today | what "real" assertion is |
| --- | --- | --- |
| cyclonedds | **implemented** — `dds_assert_liveliness` on the writer, gated on `manual_liveliness` (`src/publisher.cpp:314`) | already real: renews the writer lease and sends a Heartbeat |
| zenoh | stores `now_ms()` into `last_assert_at_ms`; drives a LOCAL `LivelinessLost` event only (`shim/publisher.rs:1006`, consumer at `:761`) | a remote-observable refresh — i.e. re-asserting the `@ros2_lv` liveliness token, or a keepalive a peer can see |
| xrce, uORB | slot NULL → runtime answers `UNSUPPORTED` for `MANUAL_*`, `OK` otherwise (documented at `rmw_vtable.h:474-480`) | out of scope; neither transport has a lease |

So the row is really **one backend's question**: what does zenoh do?

**Options for zenoh:** (a) undeclare + redeclare the liveliness token on assert
— semantically right for `@ros2_lv`, but a token churn a peer sees as
leave/join, which is worse than silence; (b) rely on zenoh's own session
keepalive, i.e. declare the manual kinds UNSUPPORTED on zenoh and let the
runtime return `UNSUPPORTED` — honest, and the ABI already specifies that
answer; (c) leave the local-event behaviour and DOCUMENT it as local-only.

**Recommendation: (b), and re-verdict the row.** zenoh-pico's liveliness model
is session-scoped keepalive; manual per-topic assertion is a DDS concept it
does not have. Answering `UNSUPPORTED` is what `rmw_vtable.h` already prescribes
for a backend without manual liveliness, and it turns a silently-wrong `Ok(())`
into a loud one. That makes the row a **`divergence`** on a platform constraint
(the transport has no per-topic lease), not a `gap` — with the note that
Cyclone does implement it, so the divergence is per backend and belongs in
`docs/reference/platform-implementation-notes.md`.

Keeping the local `LivelinessLost` event is fine and should be documented as a
local watchdog; what must stop is `Ok(())` from a call that asserted nothing
remotely.

**Risk:** medium — it changes a return code on a path some image may call
unconditionally. `rmw_vtable.h:474-480` already says a caller may do exactly
that, so check no in-tree image treats a non-OK here as fatal before landing.
**Needs a compile nobody ran:** yes, and a zenoh runtime cell.

## Row 8 — `c:node_get_graph_guard_condition` [gap, adopt]: is it even the right primitive?

**Owed:** both halves — an accessor AND a backend that triggers it. Verified
NULL in Cyclone (`vtable.cpp:456`) and uORB (`vtable.cpp:147`); zenoh does not
name the slot at all and inherits `EMPTY_VTABLE`'s `None`
(`packages/rmw/cffi/src/lib.rs:294`).

**Is it the right primitive here? Yes, and the ABI already says why.** Upstream
returns a handle you put in a wait set; we decline wait sets (RFC-0002, one
executor per RTOS task). The slot is deliberately shaped like `set_wake_callback`
— `(session, callback, user_data)`, an EDGE with no payload — and
`rmw_vtable.h:1146-1161` says so, including that the honest name would be
"set-on-graph-change-callback". Our `GuardCondition` is a real, working
executor concept: an arena `AtomicBool` plus a `CallbackMeta`, serviced by
`guard_try_process` during `spin_once`
(`packages/core/nros-node/src/executor/arena.rs:3284`), with a condvar wake on
`std` + `rmw-cffi`. So the capability has a home, and a ported file that tries
to put the handle in a wait set fails at the wait set, not here.

**The cost is wildly asymmetric, and nothing records it:**

* **zenoh: small.** The change signal already arrives. `zpico_graph_cache_start`
  declares a standing `z_liveliness_declare_subscriber` with `history = true`
  and every token — initial and subsequent — lands in
  `graph_cache_sample_handler` (`zpico-sys/c/zpico/zpico.c:4031-4086`).
  Forwarding an edge from that handler to a stored callback is the work. The
  shim exposes the cache as a POLL today (`for_each_entity` never blocks and
  has no callback out, `shim/session.rs:801-819`), so the plumbing is
  handler → session state → the slot.
* **Cyclone: not small.** Every graph reader is created with a `nullptr`
  listener and read with `dds_read` (`src/graph.cpp:359`,
  `src/graph_query.cpp:95`); there is no `dds_listener_t`, no
  `on_data_available` and no waitset anywhere in the backend. A change edge
  means introducing one of those, which introduces a callback context and its
  thread-safety question into a backend that has so far had neither.
* **uORB, XRCE:** no graph, no edge. `UNSUPPORTED` is the answer.

**Recommendation:** split the row. Ship the **accessor plus the zenoh
producer** as one item — that is a real capability on the default RMW and it
satisfies the "never hand back a guard condition nothing fires" rule for the
backend most users are on. File **Cyclone's listener** as a separate backend
item with its own issue; it is a change to Cyclone's threading model, not an
API row. Answer `UNSUPPORTED` on the rest, which the ABI already provides for.

**Flag:** the accessor is new C/C++/Rust surface over an EXISTING vtable slot,
so no ABI header change and no `gen-abi-bindings.sh` run.
**Needs a compile nobody ran:** yes, plus a zenoh graph cell.

## Row 9 — `c:lifecycle_change_state` [gap, adopt]: the expensive one, priced again

**Owed:** the `~/transition_event` publisher and the `bool publish_update`
argument that follows it. Re-confirmed: `TransitionEvent` appears only in the
`round_trip_transition_event` unit test
(`packages/core/nros-node/src/lifecycle_services.rs:947`), the generated
`nros-lifecycle-msgs` crate, and two schema/size test tables. The string
`transition_event` appears as a topic nowhere.

**The price, itemised** (the ledger's estimate is right; this is the detail):

1. A CDR writer, **~10 lines** — `TransitionEvent` is a stamp plus what
   `write_transition_desc` (`lifecycle_services.rs:306-310`) already writes.
   Cheapest part.
2. A `create_publisher` site in
   `packages/core/nros-node/src/executor/spin.rs::register_lifecycle_services`
   and a trigger hook on the transition path.
3. `EntityInventory::derive` (`packages/cli/nros-cli-core/src/entity_inventory.rs:2440`):
   `max_publishers` at `:2570` has **no infra addend today** and needs one; that
   flows into `max_liveliness` at `:2624` (one more token per lifecycle image).
4. **`check-infra-queryable-counts` cannot see it.** Its lifecycle group counts
   `create_lc_srv::<…>` sites only (`scripts/check-infra-queryable-counts.py:39`,
   `:50`); the `create_publisher` counter at `:64-71` is scoped to `action.rs`.
   So a new lifecycle publisher needs a NEW constant
   (`LIFECYCLE_SERVICE_PUBLISHERS`) and a new group, or the number lives
   nowhere the gate can hold it. That is the 0196 reach-gap shape and is the
   part most likely to be missed.
5. **If the publisher is TRANSIENT_LOCAL** — which is what REP-2002 / rcl use
   for `~/transient_event`-class topics — it also costs a **queryable** on
   zenoh (issue 1378: a transient-local publisher IS a queryable), which is the
   `tl_queryables` term at `entity_inventory.rs:2595-2605`. Decide the QoS
   BEFORE sizing anything.
6. Mirrors: `entity_inventory.rs:756-760`,
   `packages/rmw/zenoh/nros-zpico-build/src/runner.rs:39-40`.

**Recommendation:** keep `adopt`, schedule it as its own item, and make the
first decision the QoS — point 5 changes the pool arithmetic on the default RMW
and is invisible if you start from the CDR writer. Add the gate constant in the
same commit as the publisher (CLAUDE.md's issue-0196 rule: check the gate
actually covers the new site).

**Risk:** medium-high — it moves derived pool counts in every lifecycle image,
which is the class `check-c-array-pool-floors` and the sizing descriptor both
watch.
**Needs a compile nobody ran:** yes, and a lifecycle image build to confirm the
counts.

## Row 10 — `rust:Logger::set_default_level` [gap, adopt]

**Owed:** the capability, then the name — the row is right and the measurement
confirms it. `Logger::new` hard-codes `Severity::Info`
(`packages/core/nros-log/src/lib.rs:210-211`); `register_logger` (`:380`),
`get_logger` (`:391`) and `INTERN` (`:372`) consult no default; `DEFAULT_LOGGER`
(`:302`) is one ordinary logger named `"nros"`. There is no "level new loggers
start at".

**This is row 5's mechanism.** See §Sequencing and Row 5 for the sentinel and
the hot-path cost. `set_default_level` is the writer of the same `AtomicU8`
that `UNSET` reads.

**Recommendation:** build with row 5. Do NOT close by re-exporting
`DEFAULT_LOGGER.set_level` — the row names the hazard correctly and it is the
`PollingSubscription::take()` shape.

**Risk:** low. **Needs a compile nobody ran:** yes, with row 5.

## Row 11 — `rust:init_with_args` [gap, adopt-bounded]

**Owed:** the `--ros-args` parse, and nothing else. Verified:
`init_with_args` (`packages/api/nros/src/init.rs:657`) calls `refuse_ros_args`
(`:629`) → `args_have_ros_args` (`:616`, exact match, separately unit-tested) →
log + panic with `REFUSE_INIT_ARGS` (`:598`). An argv with no ROS arguments
passes through unchanged.

**Two things the measurement adds.** First, the refusal's own text names where
the parse belongs — "beside `nros::resolve_name` … RFC-0020 violation class 4"
— and class 4 is "key-expression / topic construction belongs in
`TopicInfo`/`ServiceInfo`/`ActionInfo`", i.e. name resolution is not a
wrapper's job. That is correct. Second, **most of the machinery already
exists**: `nros_node::names::resolve_name(source, node_name, namespace, remaps)`
(`packages/core/nros-node/src/names.rs:110`) takes an iterator of
`(from, to)` pairs and does the expand-compare-substitute. What is owed is the
producer of that iterator from argv, plus somewhere to store it per node.

**Recommendation: leave it, and say why on the row.** The parse is owed to a
wave that does not exist, and today remaps arrive from the LAUNCHER projected
into the environment — RFC-0046's authoritative-identity model, which is the
shipping path. Building an argv parser now creates a SECOND source of remaps
with no rule for which wins, and "two sources of truth for a node's identity"
is a larger defect than the missing parse. The correct next step is not code:
it is a one-paragraph decision on the row saying whether argv remaps are
intended to override launch-projected ones or to be refused in their presence.
Until that is written, `adopt-bounded` with a loud refusal is the right state,
and the disposition change the 2026-09-23 pass made is correct.

**Risk:** the risk is in building it early, not in leaving it.
**Needs a compile nobody ran:** not applicable; nothing to build yet.

## Row 12 — `cpp:Node::create_subscription` [gap, adopt]: the argument order

**Owed:** the open half is the native callback overload's
`(out, topic, callback, qos = default, options = {})`
(`packages/api/nros-cpp/include/nros/node.hpp:1551`, SFINAE guard `:1548-1550`)
against upstream's `(topic, qos, callback, options)`. The out-param is a
constraint (RFC-0018/0019); the order is not.

**The measurement the row was missing: it is nearly free in-tree.** Call sites
of the affected overload, whole tree:

* `packages/api/nros-cpp/tests/compile/one_node_type.cpp:124` (the 3-arg form,
  no qos)
* `packages/api/nros-cpp/tests/compile/rclcpp_node_freestanding_surface.cpp:109`
  (4-arg, with qos)

Every `.cpp` example uses the POLL-style `create_subscription(sub, "/chatter")`,
and the ported template uses the rclcpp adapter at `nros.hpp:594` (declared
`node.hpp:844`), which is already upstream's order.

**The constraint the row names is real and the way out is upstream's own.**
Putting `qos` before `callback` makes the 3-argument form illegal, because a
defaulted parameter cannot precede a non-defaulted one. Upstream solves it by
making `qos` MANDATORY. Overload resolution stays unambiguous: the 4-arg
`(out, topic, qos, callback)` and the existing poll-style
`(out, topic, qos, options)` differ by the SFINAE guard on `F` versus a
`const SubscriptionOptions&`, and a lambda converts to neither of the other's
type.

**The class, which must move together:** three siblings have the same shape and
a fix to one alone is the "fixed the reported site" failure CLAUDE.md names —
`create_subscription_with_info` (`node.hpp:1565`),
`create_subscription_with_safety` (`:1588`, behind `NANO_ROS_SAFETY_E2E`) and
`create_subscription_in_group` (`:1883`). There is no publisher twin (publishers
have no callback).

**Options:** (a) flip all four to `(…, qos, callback, options)`, dropping the
3-arg convenience form; (b) keep the order and re-verdict the row as a
`divergence` on a C++ default-argument rule; (c) ADD an upstream-ordered
overload beside the existing one.

**Recommendation: (a).** It is two in-tree edits plus three sibling signatures,
it makes the native surface agree with the adapter that already sits beside it,
and RFC-0089 clause 2 is explicit that an ordering preference is not something
to trade away. (b) is not available honestly — RFC-0036 forbids recording a
preference as a divergence, and the row already proves it is a preference by
pointing at the adapter. (c) is the worst of the three: two orders on one name
is the ambiguity a reader cannot resolve by reading.

The cost to be honest about: the 3-argument
`create_subscription(sub, "chatter", cb)` goes away, and the one place that
uses it is a compile test asserting the shape. If that convenience is wanted,
it comes back as `create_subscription(sub, topic, QoS(10), cb)` — one more
argument, upstream's spelling.

**Risk:** low in-tree, but this is a PUBLIC C++ signature and an out-of-tree
consumer pays. Worth a changelog fragment.
**Needs a compile nobody ran:** yes — `just check cpp` plus the compile tests.

## Row 13 — `cpp:Subscription::get_actual_qos` [gap, adopt]: the row that arrived mid-study

**This row did not exist when the other twelve were measured.** It was filed by
phase-456 W2b and reached this branch's base in the 88 commits between the
measurement and the commit. It is in the study because leaving it out would
make the study wrong on its own first line, and because how it arrived is
instructive: the split that created it **recorded the absence instead of
inventing a signature**, which is the campaign working, not failing.

**What is owed, measured 2026-09-25.** W2b gave the two subscription roads two
types. `nros::PollSubscription<M>` owns its subscriber and keeps the accessor
(`packages/api/nros-cpp/include/nros/polling_subscription.hpp:430`), passing
`storage_` with the old `callback_mode_` branch gone. The DISPATCH type —
`rclcpp::Subscription<M>`, which is what a ported rclcpp node actually holds —
lost it, because it holds no executor: only `sched_handle_id_`
(`packages/api/nros-cpp/include/nros/subscription.hpp:409`).

The FFI is already shaped for both roads —
`nros_cpp_subscription_get_actual_qos(storage, executor, handle_id, out_qos)`
(`packages/api/nros-cpp/include/nros/nros_cpp_ffi.h:2659`), documented as
"exactly one of `storage` / `executor` identifies a live subscription". Its Rust
side (`packages/api/nros-cpp/src/subscription.rs:1193`) takes the `storage`
branch when non-null and otherwise resolves
`ctx.executor.subscription_handle(handle_id)`.

**That last line settles one of the three options before anyone tries it.** A
`handle_id` is EXECUTOR-SCOPED — it is an index into that executor's arena, not
a process-wide identity — and RFC-0002 puts one executor on one RTOS task, so a
tiered image has several. So "resolve the arena entry from the sched handle
alone" cannot work without a global executor registry, which is a much larger
change than the row it would close.

**The options that remain:**

**(a) Store the executor beside `sched_handle_id_`.** `Node::create_subscription`
already holds `executor_handle_`, so the value is in hand at creation; the
accessor then calls the existing FFI with `(nullptr, executor, handle_id)` and
keeps upstream's no-argument spelling exactly.
*Cost:* one pointer per dispatch subscription, in caller-placed static storage.
On a board with a handful of subscriptions that is tens of bytes; it is not
free and it is not interesting.
*Risk:* low. No ABI change — the FFI signature already exists and already
supports this call shape.

**(b) Add an argument to the C++ accessor** so the caller passes the executor.
Rejected, and W2b already rejected it in as many words: it closes the gap and
opens a divergence, on a method that exists precisely to adopt the upstream
no-argument spelling. RFC-0036 does not permit recording that as a divergence,
because the constraint is one we chose.

**(c) Leave it absent.** Defensible only if a ported node never calls it. It
does — `get_actual_qos` is how a user finds out why nothing is arriving — and
the disposition the row carries is `adopt` for that reason.

**Recommendation: (a), and the decision belongs to phase-456, not here.** The
row is a consequence of that phase's split and the person holding that context
should close it; this study's contribution is the measurement that kills option
(b)-by-handle-id. Priced as the cheapest of the non-trivial rows once someone
is already in `nros-cpp`.

**Flag:** no ABI header change, no vtable, no codegen version, no `nros-rmw`
trait.
**Needs a compile nobody ran:** yes — `just check cpp`.

---

# What this touches that has a gate

Stated once, because three of the rows above are priced wrong if this is
missed.

**Changes a committed ABI header or the vtable: none of the thirteen.**
`cpp:Publisher::get_gid` uses the EXISTING `get_gid_for_publisher` slot
(`rmw_vtable.h:953`) and `c:node_get_graph_guard_condition` the EXISTING
`node_get_graph_guard_condition` slot (`:1162`). Q1 option (a) widens a
`nros-core` Rust field, which is not on the C ABI — `MessageInfo` is not
`repr(C)` and appears in no C or C++ header. So **no `scripts/gen-abi-bindings.sh`
run and no `check-abi-bindings` regeneration is owed by any row here**, which
is the opposite of what "thirteen RMW rows" sounds like.

**Needs an `NROS_CODEGEN_VERSION` bump: exactly one, and only under one
option.** Q3 option (a) — a trait in `nros-core` that codegen implements — is
the named trigger in the bump policy (`packages/core/nros-core/src/codegen_version.rs:27-31`).
The four files, measured off the last five bump commits:

1. `packages/core/nros-core/src/codegen_version.rs` (the constant, and a
   paragraph justifying the `_MIN` floor)
2. `.config/codegen-version-surface.txt`
   (`python3 scripts/check-codegen-version-surface.py --write-baseline`)
3. `packages/api/nros-c/include/nros/nros_config_generated_nuttx.h` — **two
   arms of the same file**
4. `packages/api/nros-cpp/include/nros/nros_cpp_config_generated_nuttx.h`

plus re-blessing the golden fixtures that carry `NROS_EMITTED_CODEGEN_VERSION`
— 20 of the 44 files under
`packages/cli/rosidl-codegen/tests/fixtures/fingerprint-corpus/expected/`,
measured 2026-09-25. Gates:
`check-codegen-version-surface` and `check-config-fallback-macros`. Q3 options
(b) and (c) avoid the bump and are blocked by issue 1428 instead.

**Touches `packages/core/nros-rmw` traits: exactly one row.** Q2's
`serialization_format`. Every backend recompiles; the workspace recompiles.
Measured 2026-09-25: 17 manifests under `packages/` take a direct `nros-rmw`
dependency, and `nros-node` — one of them — sits below nearly everything else,
so the transitive set is most of the workspace. No gate compares the Rust trait to the C slot —
the only enforcement linking them is the compile-time const equality on
`SERIALIZATION_FORMAT{,_ID}` against the generated macros
(`packages/api/nros-c/src/constants.rs:127-132`,
`packages/api/nros-cpp/src/lib.rs:5847`), which an `Option` on the METHOD
does not disturb.

**One gate gap worth filing separately:** `check-infra-queryable-counts` cannot
see a lifecycle PUBLISHER (Row 9, point 4). That is true today, before anyone
writes the publisher, and it is a gate whose reach is narrower than the rule it
enforces.

# Corrections owed to other documents

None of these is work; all are one-line edits, and each is a claim that is
currently false.

1. **`docs/reference/api-parity-ledger/qos.json`** — `cpp:RosoutQoS` cites
   `qos.hpp:745`; the class is at `:977`. The same row cites `options.hpp:250`
   for the `enable_rosout` refusal, which is at `:256`. And its `disposition`
   field now reads `adopt` while its prose still says "ADOPT-BOUNDED on exactly
   that point" — the field moved and the reason did not. Nothing gates a
   disposition against its own reason, so that disagreement is silent.
2. **`docs/reference/api-parity-ledger/pubsub.json`** —
   `cpp:Node::create_subscription` cites `node.hpp:1458` and guard
   `:1455-1457`; they are `:1551` and `:1548-1550`.
3. **`docs/reference/api-parity-ledger/timer.json`** — `rust:Time::to_ros_msg`
   cites `cargo-nano-ros/src/lib.rs:720` for `bundled_interface_packages()`;
   it is `:790`. The same row's "there is no crate for the return type to come
   from" is superseded by issue 1428 and should point at it.
4. **`docs/reference/api-parity-ledger/pubsub.json`** —
   `cpp:Publisher::assert_liveliness` reads as though no backend asserts.
   Cyclone does (issue 1231, archived); the finding is zenoh-specific.
5. **`docs/reference/api-parity-ledger/node.json`** —
   `c:node_get_graph_guard_condition` says "the inputs exist and nothing turns
   them into an edge". True, but the zenoh inputs are a standing SUBSCRIBER
   with history and Cyclone's are polled reads with `nullptr` listeners, which
   is a large asymmetry in what "turning them into an edge" costs.
6. **[phase-417](phase-417-ros2-api-adoption.md)** §"(4) Our own three
   languages disagree" lists `cpp:Publisher::get_gid` under "Rust exposes
   `MessageInfo::publisher_gid`, C++ does not". The 2026-09-23 re-read refuted
   exactly that: no language exposes a publisher's own gid, and
   `MessageInfo::publisher_gid` is a different identifier. The family table
   should drop the row or restate it.
7. **`packages/api/nros-c/include/nros/log.h:61-63` and
   `packages/api/nros-c/src/log.rs:85-87`** say
   `rcutils_logging_set_logger_level(name, UNSET)` "UNSETS a logger's level".
   The Humble `set_logger_level` doc block does not say that (Row 5, above);
   the sentence belongs to `set_default_logger_level`. Narrow both comments to
   the ancestry-walk fact they can cite, or read `logging.c` and cite that.
   This is the only correction in this list that is currently telling a USER
   something upstream does not guarantee.
8. **`packages/core/nros-log/README.md:24`** still says
   `sinks::default()`; `packages/core/nros-log/src/sinks.rs` has been empty of
   sinks since issue 0710 and tells callers
   `nros_log::init(nros_platform_cffi::log::default_sinks())`.

# What this study did not do

* **No build was run**, deliberately. Every row above that would need one says
  so. Ten of the thirteen cannot be closed without a compile, and three of the
  four decisions change a signature whose only real cost is that compile.
* **No live peer.** Q1 option (b) and Row 7's zenoh half both need the
  router-and-peer lane [phase-444](phase-444-rmw-fix-up.md) W2 still owes, and
  neither should be attempted from reading.
* **`rcutils/src/logging.c` was not read**, because it is not on this machine
  and getting it needs either apt sources (`sudo`) or a fetch from upstream.
  Row 5 says exactly which sentence in our own header depends on it.
* **Every upstream header quotation here was read INSIDE the `ros2` box.** The
  host has no `/opt/ros`; a search run on the host reaches a different world
  and can land on a Foxy copy vendored somewhere unrelated. The box rule is
  about reading as much as about building.
* **No ledger edits.** The corrections above are listed, not applied, so that
  each lands with the person who owns the shard — the same rule
  `c:logging_rosout_enabled` applied to itself when it flagged the `/rosout`
  family rather than re-verdicting two rows in another shard.
