# Phase 379 — the user API is rclc / rclcpp / rclrs, and something checks that

**Status (2026-08-24). W1 LANDED — the correlator runs on all three languages
and its first report is below.** No API has been corrected yet; W1 exists to
make the corrections findable and to stop the next one being invisible. W2–W5
are the corrections and are not started.

**Implements.** RFC-0036 (divergences from the ROS 2 standard client APIs),
which this phase converts from a prose catalog into a checked one. Touches
RFC-0018 (C++ API design), RFC-0019/0020 (thin-wrapper discipline), RFC-0022
(entity API tiers), RFC-0037 (Rust/C user API surface).

## Why

nano-ros makes a drop-in claim: a ROS 2 developer can read and write it, and a
ported source file compiles against it with a build-glue change rather than a
rewrite. Phase 209 built the C++ half of that (`nros/rclcpp_compat.hpp`, the
`cmake/compat/include/rclcpp/` shim, `Findrclcpp.cmake`), so a `.cpp` that says
`#include <rclcpp/rclcpp.hpp>` really does compile here.

The claim is only worth something if the SHAPE underneath matches, and nothing
checked that it did. RFC-0036 is the catalog of the divergences we permit — and
it is prose. Prose about an API goes stale silently: RFC-0036 shipped calling
the Rust error type `RclrsError` when it had been `NanoRosError` for months, and
had to carry a "naming note" correcting itself. Issue 0338 is the same class one
level down: `Executor::spin` meant the OPPOSITE of `rclcpp::Executor::spin` here
(bounded, not blocking), so a user who wrote `exec.spin()` got a compile error
and the nearest-looking alternative `spin(ms)` silently returned early. That was
found by a person reading, once.

So the deliverable is a correlator: extract both surfaces from their real
sources, line them up, and report every item that does not correspond.

## W1 — the correlator (landed)

`scripts/api-parity.py`, with the extractors under `scripts/api_parity/`.

    scripts/api-parity.py                 # report all three languages
    scripts/api-parity.py --lang cpp      # one language
    scripts/api-parity.py --check         # fail on anything unledgered
    scripts/api-parity.py --suggest-renames   # pair look-alike unmatched names
    scripts/api-parity.py --include-internal  # compare the whole ROS 2 surface
    scripts/api-parity.py --refresh …     # re-derive the ROS 2 side
    scripts/api-parity.py --self-test

### How each side is obtained

Both sides are parsed, never grepped. The question the campaign asks is "do the
ARGUMENTS agree", and arguments are exactly what a regex over headers gets
wrong — default values, template parameters, `const &` versus value, and
macro-expanded visibility attributes (`RCLCPP_PUBLIC`) all defeat it.

| lane | ours | theirs |
| --- | --- | --- |
| C++ | `nros/nros.hpp` via clang JSON AST | `rclcpp` + `rclcpp_action` + `rclcpp_lifecycle` from `/opt/ros/<distro>` |
| C | `nros/nros.h` via clang JSON AST | `rclc` checkout **plus `rcl`** |
| Rust | rustdoc JSON over the `nros` facade | rustdoc JSON over `rclrs` |

Three things about that table are decisions rather than mechanics:

**Our side parses with no build.** `-DNROS_PLATFORM_NUTTX` selects the
COMMITTED size header (`nros_cpp_config_generated_nuttx.h`); every other
platform's sizes come from `build.rs`, which would make this tool depend on a
fixture being fresh. Both our surfaces parse with zero clang errors, and that is
enforced rather than tolerated — a partial AST silently drops declarations, and
a dropped declaration reads as a gap in our surface that is not really there.

**The C reference is rclc AND rcl.** rclc is a convenience layer, not a whole
API: its own examples call `rcl_publish`, `rcl_take` and `rcl_*_fini` directly
(`rclc_examples` has 23 `rclc_executor_init` calls against 6 `rcl_publish`).
Comparing against rclc alone scored our publish and take entry points as
inventions when they are the ROS 2 C API doing its job — 129 reference records
became 747 once `rcl` was included.

**The ROS 2 side is cached, and re-derivable.** `docs/reference/api-surface/*.json`,
for the reason `scripts/rmw-api-parity.py` caches its contract: the comparison
must run on a host with no ROS, no rclc checkout and no rclrs workspace, or it
runs on one host and rots everywhere else. Each file records its provenance
(distro, git ref, crate version). OUR side is never cached — caching it would
defeat the tool, which exists to notice when an edit moves us away from ROS 2.

### Only PUBLIC ROS 2 items are compared

nano-ros aligns to the API a ROS 2 user writes. It does not align to rclcpp's
callback type erasure, rcl's wait-set plumbing, or the generated accessors of
`rcl_interfaces`, and counting those as gaps manufactures work that should never
be done.

`public_surface.py` decides on the DECLARING FILE, not the name — a path is a
fact about the library's own organisation, a name is a guess about intent.
`AnyExecutable` looks internal and is; `Waitable` does not and is; their headers
say so either way. Three tiers, in order of how much judgement each needs:
generated message packages (`*_msgs`, `*_interfaces`, `rosidl_*`), `detail/`
directories (upstream's own marker), and a short enumerated list of plumbing
headers each carrying the reason a user never writes it.

The first tier alone is **216 of the C lane's 632** `theirs-only` rows — every
`rcl_interfaces__srv__GetParameters_Request__init` and its family. Those are
codegen output on BOTH sides, governed by RFC-0023/0033; comparing them compares
two code generators, not two APIs.

The report always prints what each tier removed. A filter that quietly shrinks a
number is indistinguishable from progress. `--include-internal` turns it off.

### Systematic signature changes are stated once, not per site

Some divergences are not per-item decisions — they are one decision applied
everywhere. `rcl` threads an `rcl_allocator_t *` through six entry points;
nano-ros has one global allocator, so it appears in none of them. That is one
sentence, and writing it into six ledger rows is how the sentence stops being
read.

`signature_rules.py` holds those rules, each with the constraint it answers and
the entry points it covers. A divergence a rule explains is bucketed
**`systematic`** and inherits the rule's constraint; only what NO rule explains
stays `differs` and needs a ledger row. The five rules, all read off the first
report rather than guessed:

| rule | constraint |
| --- | --- |
| `no-allocator` | one global allocator (`nros_platform_alloc`, gated by `check-no-direct-kernel-alloc`) — a per-call allocator argument could only be passed the same value or a wrong one |
| `compile-time-options` | QoS and entity options are selected at compile time (RFC-0036, RFC-0045); accepting an options struct would promise a negotiation the backends do not perform |
| `no-argv` | an embedded image has no argc/argv; boot config is baked |
| `executor-owns-no-entity-storage` | the callback and message buffer bind to the ENTITY at creation (RFC-0041), so the executor has no per-entity storage to be told the size of |
| `handle-owns-node` | our entity handles retain their node, so teardown does not ask the caller to still hold it — one pointer per entity against a lifetime the caller would otherwise enforce with no allocator and no ownership types |

Matching drops the FEWEST parameters that explain the difference, in priority
order, stopping the moment the arities overlap. Applying every rule at once
over-explains: `rcl_publisher_init` takes both an options struct and a node,
ours takes the node and not the options, and dropping both leaves theirs one
parameter shorter than ours — a difference invented by the explanation. The
report names the rules actually needed.

Deleting a rule re-opens every row it covers, which is the intended way to
challenge one.

### What the tool refuses to do

**It does not use an authored name map.** A map for ~2000 items is a document
nobody finishes and nobody re-reads. Names already correspond by construction —
that is the project's stated goal — so the tool ASSUMES correspondence and makes
disagreement the thing a human has to write about. That puts the labour on
exactly the rows the campaign cares about.

**It does not compare full types by default.** A type difference is usually
RFC-0018's `std::string` → `const char *` rule applied again; reporting those
would bury the real findings under hundreds of rows restating a decision made
once. Arity is the primary comparison, because an arity difference means the two
APIs ask the user for different things. Full parameter lists print alongside.

### Four tool defects found before trusting the output

Each produced findings that looked real. They are recorded because the tool's
credibility is the deliverable:

1. **File-based scoping.** clang emits `loc.file` only when it CHANGES, so
   recovering a decl's file means carrying state across a strict pre-order walk
   of a 400 MB AST. Getting it subtly wrong attributed `std::shared_mutex` and
   `builtin_interfaces::msg::Time_` to rclcpp while dropping `rclcpp::Node`
   entirely. Fixed by scoping on NAMESPACE, which is already on the path down
   and cannot drift.
2. **Single-crate rustdoc.** rustdoc writes one JSON per crate, and a
   re-exported item's id belongs to the crate that DEFINED it. `nros` is a
   facade, so without cross-crate resolution the entire executor, node-context
   and publisher surface read as absent — 168 items instead of ~750.
3. **Default arguments not counted.** clang marks a defaulted parameter with
   `"init": "c"` and attaches the default expression as a child of whatever
   literal was written — `IntegerLiteral` for `= 10`, not something ending in
   `Expr`. Counting declared parameters reported
   `nros::Executor::spin(int32_t poll_ms = 10)` as diverging from
   `rclcpp::Executor::spin()`, when `exec.spin()` compiles in both — which is
   precisely the convergence issue 0338 landed on purpose. **A checker that
   flags a convergence someone deliberately made is worse than no checker.**

4. **rclcpp's inheritance split read as divergence.** rclcpp splits every entity
   into a type-erased base and a typed subclass — `Publisher<T>` IS-A
   `PublisherBase`, and `get_topic_name`, `assert_liveliness`,
   `wait_for_service` and `cancel` are declared on the base. `nros::Publisher`
   is one class, so those appeared as an `ours-only` row and a `theirs-only` row
   that never mentioned each other. Folded the suffix, as the rclrs `XState`
   split already was. Note the fold has to reach the TYPE key and not only a
   method's owner: member keys are built from the type key, so folding the
   owner alone changes nothing — which it did, silently, until the numbers
   refused to move.

Before defect 3 was fixed the C++ lane reported 11 argument divergences. After
it, zero. All eleven were the tool's. Defect 4 then moved 7 more rows into
`same` and removed 35 phantom `theirs-only`.

## The first report

    same       both sides have the name and their arities overlap
    systematic they do not, and a signature rule explains it
    differs    they do not, and NOTHING explains it
    +          ours only
    -          theirs only

Against the PUBLIC ROS 2 surface only:

| lane | reference | same | systematic | differs | ours-only | theirs-only | excluded as not-API |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| C++ | rclcpp (humble) | 61 | 0 | **0** | 217 | 766 | 2 message, 12 plumbing |
| C | rclc+rcl (humble) | 67 | 24 | **8** | 306 | 385 | 216 message, 33 plumbing |
| Rust | rclrs 0.5.1 | 44 | 0 | **0** | 709 | 327 | 2 plumbing |

The C++ and Rust lanes show 0 `systematic` because they show 0 `differs` — no
rule is ever consulted. The out-parameter rule (`status-return-out-param`) is
carried for when a C++ row does diverge; it fires nowhere today, and the phase
doc says so rather than implying the rule earned its place.

Each language has a different problem, and none of them is the one the campaign
was opened to fix.

**C++ — the shape is right; the COVERAGE is not.** Zero argument divergences.
Every name we share with rclcpp, we spell the same and accept the same arity.
What is missing is surface: `Node::declare_parameter`, `Node::create_wall_timer`,
`Node::get_clock`, `Node::count_publishers`, `Node::get_fully_qualified_name`,
and types a ported node names directly — `Clock`, `Duration`, `Context`,
`HistoryPolicy`, `DurabilityPolicy`, `FutureReturnCode`, `CallbackGroupType`.
The 804 includes rclcpp internals a user never writes (`AnyExecutable`,
`GenericRate`, the memory strategies), so it is an upper bound, not a work list
— W2 turns it into one.

**C — 32 argument divergences, of which 24 are five decisions and 8 are open.**
The rules above cover the allocator, the options structs, argv, the
executor's entity storage and the node-carrying handles. What remains is the
real work list, and it is specific:

* `timer_get_period` — ours RETURNS the period, rcl writes it through an
  `int64_t *`. The inversion is ours and has no stated reason.
* five `lifecycle_*` entry points — ours take
  `struct Option_LifecycleCallbackFnCtx` plus a `void *` context where rclc
  takes a bare `int (*)(void)`. A Rust `Option<...>` type name reaching the C
  ABI is its own finding, separate from the arity.
* `lifecycle_change_state` — rclc takes a trailing `bool`; we do not.
* `make_node_a_lifecycle_node` — ours takes 2 parameters against rclc's 5.
* `action_publish_feedback` — ours `(server, goal, buf, len)` against
  `(goal_handle, void *)`.

Eight rows, each needing one decision. That is a tractable W4, where 32 was a
survey.

**Rust — we EXPORT far too much.** 709 items the `nros` facade makes public that
rclrs has no equivalent for: `BOOT_SET_DOMAIN`, `BakedBootConfig`, `BoardConfig`,
`ActionExecutor`, `CallbackCtx`, `ActionTag`, and hundreds more. Some are
genuine RTOS extensions and belong. Many are internals that reached `pub use`
because a facade re-exports whatever it is handed. A user reading `nros::` to
learn the API meets all 709, which is its own kind of divergence from rclrs.

Note the reference: **rclrs 0.5.1**, while RFC-0036 says we target 0.7.0. 0.5
introduced the `Node = Arc<NodeState>` split that the correlator has to fold
(the methods live on `NodeState`; a user writes `Node`). Which version we mirror
is a decision W5 has to make and record, not a detail.

## W2 — turn `theirs-only` into a work list, per language

The counts above mix "a gap we should close", "a decline we should state" and
"an rclcpp internal that is not API". Nobody can act on the mixture. W2
classifies every non-matching row into `docs/reference/api-parity-ledger.json`
with one of five verdicts, each requiring a written reason:

* `divergence` — we changed it and a PLATFORM CONSTRAINT is why. The reason must
  name the constraint (`no_std`, no exceptions, no allocator, no runtime env,
  single-threaded transport), not a preference. This is the only sanctioned
  reason to differ.
* `extension` — we add it because an RTOS scenario needs it.
* `declined` — ROS 2 has it, we deliberately do not, with the reason.
* `gap` — ROS 2 has it, we should too, nobody has done it. A gap is a legitimate
  ledger entry; the point is that it is WRITTEN DOWN.
* `rename` — the names differ and OURS is the one that should change. This is
  the campaign's work list, because a rename with no platform reason costs the
  drop-in claim for nothing.

Acceptance: `scripts/api-parity.py --check` is green, and joins the `just check`
fast lane. Until then the gate is not wired — a gate that fails on ~2000 rows
from the day it lands is one somebody switches off.

## W3 — close the C++ coverage gaps a ported node actually hits

Driven by W2's `gap` rows, ordered by what phase 209's port templates and the
autoware survey nodes call. Expected shape: `create_wall_timer` as a name
alongside `create_timer`, `declare_parameter` over the current parameter
surface, `get_clock`/`Clock`/`Duration`, the QoS policy enums under their rclcpp
names.

`--suggest-renames` already names the cheapest ones, and they are cheap because
none has a platform reason: `Service::send_reply` against rclcpp's
`send_response`, `Service::try_recv_request` against `take_request`,
`Subscription::try_recv` against `take`, `make_publisher`/`make_subscription`
against `create_publisher`/`create_subscription`, and `Timer::is_cancelled`
against `is_canceled` — a spelling. The QoS accessors (`deadline_ms`,
`lifespan_ms`, `liveliness_lease_ms`) are a different case: the `_ms` suffix
encodes that we take an integer where rclcpp takes a `Duration`, so the name
follows whatever W3 decides about `Duration`, not the other way round.

## W4 — settle the eight open C divergences

The 24 systematic rows are already argued, in `signature_rules.py`; W4's job
there is only to check each constraint still holds and move the text into
RFC-0036. The eight above each need a decision: a `divergence` row naming a
constraint, or a signature change. No row survives as "that is just how it is".

`Option_LifecycleCallbackFnCtx` is the one to look at first — a generated Rust
type name in the C ABI is a leak regardless of what the arity comparison says
about it.

## W5 — the Rust facade, and which rclrs we mirror

Two decisions, neither of them mechanical:

* Which rclrs version is the target — 0.5.1 (what exists here) or 0.7.0 (what
  RFC-0036 claims). They differ in the `Node`/`NodeState` split, which changes
  what "matching rclrs" even means.
* What `nros::` should export. 709 items is not a surface a user can read. The
  likely answer is a `nros::prelude` that IS the rclrs-shaped API and an
  explicit second tier for the RTOS-specific machinery — but that is RFC work,
  not a rename sweep.

## Acceptance

* W1: `scripts/api-parity.py --self-test` green; the report above reproduces on
  a host with ROS Humble, an `ros2/rclc` checkout and an `rclrs` checkout.
  **Met.**
* W2: `--check` green and wired into `just check`.
* W3: a phase-209 port template compiles without the compat header supplying a
  name rclcpp already has.
* W4: every C `differs` row carries a verdict; RFC-0036 gains the ones that are
  divergences.
* W5: an RFC recording the rclrs target version and the facade's export policy.

## Notes for whoever picks this up

* Re-derive before believing a stale count: `scripts/api-parity.py --refresh
  --rclc <checkout> --rclrs <crate dir>`. The recorded surfaces carry their
  provenance so a mismatch with your ROS install is visible.
* The 766 / 385 / 327 `theirs-only` counts are the PUBLIC surface we do not
  have. They are still upper bounds — a row can be a legitimate `declined` —
  but they no longer contain generated messages or library plumbing.
* `--include-internal` restores the unfiltered comparison. Use it when checking
  whether the public-surface rule dropped something it should not have; do not
  quote its numbers.
* `--suggest-renames` pairs unmatched names by SIMILARITY. It is the fastest
  route into W2 (it finds `send_reply` -> `send_response`, `try_recv_request`
  -> `take_request`, `try_recv` -> `take`, `make_publisher` ->
  `create_publisher`, `is_cancelled` -> `is_canceled`) and it also pairs
  `Timer` with `Time`. Suggestions never satisfy `--check`; a human confirms
  each pair and writes the ledger row.
* `--show all` prints matching rows too, which is the fastest way to check
  whether a name you are about to add already correlates.
