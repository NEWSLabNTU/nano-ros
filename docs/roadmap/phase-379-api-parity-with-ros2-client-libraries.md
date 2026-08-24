# Phase 379 — the user API is rclc / rclcpp / rclrs, and something checks that

**Status (2026-08-24). W1 LANDED — the correlator runs on all three languages
and its first report is below. W2 is READY: 1682 decisions across 16 stages,
one stage per feature, each stage covering C, C++ and Rust together and owning
one ledger shard so no two collide.** No API has been corrected yet; W1 exists to make the corrections
findable and to stop the next one being invisible. W3–W5 are the corrections
and are not started; W3 and W4 depend on W2's classification, W5 on a decision
recorded below.

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
    scripts/api-parity.py --topic pubsub              # one stage, all languages
    scripts/api-parity.py --by-topic                  # what each stage owes
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

Three kinds of rule, because there are three ways one decision shows up:

* **A dropped parameter class** — the five above. Reconciles an arity.
* **A type substitution** (`TYPE_EQUIVALENCES`) — the arity already matches and
  one position is spelled differently on each side: `const char*` against
  `const std::string&` (`cstr-not-string`, RFC-0018), a value or reference
  against a `SharedPtr` (`no-shared-ptr`, RFC-0022), `&mut Self` against
  `&Arc<Self>` (`no-arc-self`), `int` against `size_t` (`sized-integer`).
  Without these, `create_subscription`, `create_service`, `create_client`,
  `publish`, `QoS::keep_last` and a dozen more each read as an unexplained
  difference when between them they are three sentences.
* **An alignment** (`ALIGNMENTS`) — the two signatures agree about everything
  except where the RESULT goes. `create_publisher(Publisher<M>& out, const
  char*, const QoS&)` against `create_publisher(const std::string&, const QoS&,
  const PublisherOptions&)`: same arity, every position off by one. Comparing
  in place finds no agreement; comparing after the shift finds two.

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

    same       both sides have the name, and the arguments agree
    arity-only the arities overlap and NOTHING else does -- the agreement is in
               the count. `init` is the example: ours takes (locator, domain),
               rclcpp's takes (argc, argv), and both take two.
    systematic the arguments differ and a signature rule explains it
    differs    the arguments differ and NOTHING explains it
    +          ours only
    -          theirs only

Against the PUBLIC ROS 2 surface only:

| lane | reference | same | arity-only | systematic | differs | ours-only | theirs-only |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| C++ | rclcpp (humble) | 44 | 9 | 8 | **0** | 217 | 766 |
| C | rclc+rcl (humble) | 67 | 0 | 24 | **8** | 306 | 385 |
| Rust | rclrs 0.5.1 | 40 | 3 | 1 | **0** | 709 | 327 |

Not-API excluded: 216 message + 33 plumbing (C), 2 + 12 (C++), 2 (Rust).

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

## W2 — classify every non-matching row, in parallel

**`types`, `init`, `node`, `pubsub`, `service` and `timer` are DONE
(2026-08-24)** — 862 authored rows covering six stages in all three languages.
Each of the first four corrected something the stage was standing on:

* `types` found the taxonomy filing by NAME when the DECLARING HEADER was
  available and better (see below), and produced issue 0783 — the Rust facade
  exports `NodeError` but not `TransportError` or `RclReturnCode`, and
  RFC-0036's Errors row names a type the user API never returns.
* `init` found the correlator calling `nros::init(const char*, uint8_t)` the
  SAME as `rclcpp::init(int, char**)` because both take two parameters. That
  produced the `arity-only` bucket, four type-substitution rules and an
  alignment rule (below), which between them reclassified 9 rows across the
  other stages as `systematic` rather than leaving them silently `same`.

* `node` found the header rule filing METHODS by their type. `rclcpp/node.hpp`
  declares `Node::declare_parameter`, `Node::count_publishers`,
  `Node::create_wall_timer` and `Node::get_clock`, and all four were in the node
  stage — 70 rows in the wrong place. A member now asks the NAME first, matched
  against the whole key so the owning type disambiguates (`Executor::shutdown`
  is exec's, `LifecycleNode::on_shutdown` is a lifecycle transition, and the
  bare member name says `init` for both).

* `pubsub` found the rclrs `State` fold applied to a method's OWNER but not to
  the TYPE, so `rclrs::PublisherState`'s members were keyed `PublisherState::*`
  and never met ours. The C++ `*Base` fold had the same bug and was fixed in the
  first report; the Rust half survived because nothing had exercised it. Fixing
  it closed 109 decisions across every stage at once — `same` had been
  understated across the whole Rust lane.

Counts taken before 2026-08-24 will not match: all four fixes moved rows
between stages and between buckets.

### What the `node` stage established, and a correction

The Rust user API is **not rclrs-shaped**, and the first report's summary of it
was too simple. It is a declarative component model (RFC-0043/0044): a user
implements `Node` — which is a **trait** — and `ExecutableNode` for their own
type, declares entities in `register`, and receives callbacks. rclrs holds an
`Arc<Node>` and calls methods on it.

    impl Node for Talker {
        fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
            let mut node = ctx.create_node(NodeOptions::new("talker"))?;
            let pub_chatter = node.create_publisher_for_topic::<StringMsg>("/chatter")?;
            ...

The constraint is static entity storage plus an executor that owns dispatch:
with no allocator the entity set has to be known at declaration time, and with
one executor per RTOS task the callback cannot be a closure the node keeps.

So the earlier reading — "the facade exports 709 items rclrs has no equivalent
for, many of them internals that reached `pub use`" — was half right. A large
share is the component model, which is deliberate and correct. What IS a
problem is that it is not distinguishable from the machinery beside it: issue
0784 records that `nros::` publishes the component API, the machinery
`nros::node!` expands into, and four types with zero consumers, under one
namespace with nothing marking which is which. `nros::Node` is the trait; the
handle is `NodeCtx`, which the facade never exports.

**1682 decisions across 16 stages.** They parallelise cleanly because a decision
is a sentence about one item and touches nothing else. This section is written
so several agents can run at once without meeting — but the ORDER matters more
than the parallelism: the campaign closes one feature at a time, in every
language, and a stage is done only when all three lanes are.

### The unit of work is a TOPIC, in all three languages at once

A stage is a feature — node, pubsub, service — and it is finished when it is
finished in C, C++ **and** Rust. Splitting by language instead would let C++
pubsub land while C pubsub sits unexamined, and the drop-in claim is made per
language: a feature that works in one is not a feature.

Each stage owns exactly one file, `docs/reference/api-parity-ledger/<topic>.json`,
holding that topic's rows in every language. Two stages never touch the same
file, so no stage can lose another's work to a rebase. The loader merges every
`*.json` in that directory, and `--self-test` rejects a row whose item belongs
to a different stage — using the same `topics.topic_of` the report groups by, so
a shard cannot disagree with the taxonomy.

Get a stage's rows, and see what is left:

    scripts/api-parity.py --topic pubsub    # one stage, all three languages
    scripts/api-parity.py --by-topic        # what every stage still owes

    stage             c      cpp     rust    total
    types             0        0        0        0   done
    init              0        0        0        0   done
    node              0        0        0        0   done
    pubsub            0        0        0        0   done
    service           0        0        0        0   done
    timer             0        0        0        0   done
    qos              27       61       17      105
    param            79       49       50      178
    action          155       22        8      185
    exec             61       59       33      153
    lifecycle        54       45       16      115
    log              16        5       38       59
    graph            20       21       15       56
    serde            31        0        7       38
    boot              2        0       10       12
    other            12       18       70      100
                                              1001

`--by-topic` counts DECISIONS, not rows: a member whose type already carries a
verdict is answered, so counting rows would report the same work several times
and make a finished stage look unfinished.

**Stage order** is `topics.STAGE_ORDER`, and it is not arbitrary: nothing can be
complete before the entry point that creates it, and every entity is created on
a node. `types` first because every other stage's signatures are written in its
vocabulary (`nros_ret_t`, `Result`, `Expected`, the callback typedefs); then
`init`, `node`, and the entities.

**Which topic an item belongs to is decided by "which feature is incomplete
without this?"**, not by which type declares it. So `Node::create_publisher` is
pubsub — the verb exists to produce a publisher, and an audit of the publisher
API needs the way you obtain one — while `Node::get_name` is node.
`Node::declare_parameter` is param, `Node::create_wall_timer` is timer,
`count_publishers` is graph. Each of those is a test in `topics.py`, so
reordering the patterns breaks the tests rather than silently re-filing hundreds
of rows.

**The DECLARING HEADER decides before the name does.** A C API spells everything
`lower_snake_t`, so a name pattern broad enough to catch `nros_ret_t` also
catches `rcl_bool_array_t`, `rcl_topic_endpoint_info_t` and
`rcl_jump_threshold_t` — filing the YAML parameter parser, a graph query and a
clock callback under "types". That is what the first taxonomy did, and it is
invisible in the counts. The header says what the name cannot, and every record
already carries it. Names remain the fallback for headers no map should bother
with: rclcpp's `utilities.hpp` holds `ok`, `shutdown` AND `spin`, which are two
topics.

Our own C surface is the one case neither settles — cbindgen emits it all into
one `nros_generated.h` — so `topics.KEY_OVERRIDES` names those individually,
each resolved by reading the declaration.

**`other` is 228 and reported, not hidden.** A large `other` means the taxonomy
is wrong for part of the surface. The first pass had 316 and it turned out to be
three nameable things — `serde` (the CDR primitives), `types` (error, result and
callback vocabulary) and `boot` (baked boot/board config) — which are now stages
of their own. What is left is mostly the RUST lane's 134, and that is not a
taxonomy failure: it is the facade over-export this phase already identified,
and W5 owns it. Whoever takes `other` should expect to split it again rather
than classify it as it stands.

### One row per DECISION, not per symbol

The raw report has 2714 non-matching rows; 1007 of them are members of a type
that is itself unmatched. `rclcpp::Node` has 49 public methods we do not have,
and writing 49 sentences that each say "we have no Node" is a copy-paste
exercise whose fiftieth reader stops reading. So:

* **A row on a TYPE covers its members** — `cpp:Node` covers `cpp:Node::*` — but
  only while the type sits in the SAME bucket as the member. If we ship `Node`
  and lack `Node::declare_parameter`, the type is `same`, the method is
  `theirs-only`, and that is a different claim which must be argued on its own.
  An inherited verdict prints with a trailing `*`.
* **A glob row covers a family** — `c:action_*`. The C surface needs this and the
  others do not: C names are flat, so `publisher_init` and `publisher_fini`
  share no owning type for a verdict to descend from. A glob **must declare the
  bucket it covers** (`"bucket": "theirs-only"`), because otherwise one row
  would absorb a gap, an extension and an unexplained signature change alike —
  three claims under one sentence, which is the failure a ledger exists to
  prevent. The most specific pattern wins, and an exact row always beats a glob.
  This is why the C column is the tall one — 148 action decisions, 89 pubsub —
  and also why it collapses fastest once the families are named.

### The verdicts

* `divergence` — we changed it and a PLATFORM CONSTRAINT is why. The `why` must
  NAME the constraint (`no_std`, no exceptions, no allocator, no runtime env,
  single-threaded transport, static entity storage). "We preferred it this way"
  is not a divergence; it is a `rename` or a `gap` wearing a better coat. If the
  constraint you are about to write applies to more than a handful of sites,
  stop — it is a **signature rule** (see below), not a ledger row.
* `extension` — we add it because an RTOS scenario needs it. Name the scenario.
* `declined` — ROS 2 has it, we deliberately do not. Name what a user loses.
* `gap` — ROS 2 has it, we should too, nobody has done it. A gap is a legitimate
  entry; the point is that it is written down, not that it is absent.
* `rename` — the names differ and OURS should change. A rename with no platform
  reason costs the drop-in claim for nothing.

`--suggest-renames` pairs unmatched names by similarity and is the fastest way
into a stage. It is a guess: it finds `send_reply` → `send_response` and it also
pairs `Timer` with `Time`. Confirm each before writing the row.

### Rules of engagement for a parallel run

* **W2 writes ledger rows. W2 does not change any API.** The renames it
  identifies are W3/W4/W5's work, on a tree where the classification is already
  agreed. An agent that starts renaming while four others classify produces a
  report nobody can reconcile.
* **A stage is not done when one language is done.** `--topic <name>` prints all
  three lanes for a reason; finishing cpp and leaving c is how the drop-in claim
  ends up true in one language and false in another.
* **`signature_rules.py`, `public_surface.py` and `topics.py` are SHARED.** If a stage finds
  a systematic pattern or a mis-filtered internal, do not edit those files —
  say so in the stage's report and let one person land it. A rule change moves
  rows in every lane at once.
* **Do not run `--refresh`.** It rewrites the recorded ROS 2 surfaces, which
  every other stage is reading.
* **Reserve issue ids with `just issue-new <slug>`**, never by reading the
  highest number — parallel sessions collide on this and have seven times.
* **Gate before finishing:** `just check-api-parity-ledger` (buildless, seconds).

### Acceptance

Per stage: every row `--topic <name>` selects, **in all three languages**,
carries a verdict — directly, inherited from its type, or from a declared glob —
and `just check-api-parity-ledger` is green. `--by-topic` shows the stage at
`0 0 0`.

For W2 as a whole: `scripts/api-parity.py --check` is green and joins the
`just check` fast lane. Until then the gate stays out — one that fails on 1707
rows from the day it lands is one somebody switches off.

## W3 — close the C++ coverage gaps a ported node actually hits

Driven by W2's `gap` rows, ordered by what phase 209's port templates and the
autoware survey nodes call. The `types` stage has already produced two:
`cpp:FutureReturnCode` (we express SUCCESS and TIMEOUT, not INTERRUPTED, so a
ported `spin_until_future_complete` caller cannot tell shutdown from timeout)
and `rust:RclReturnCode` (we have the type and do not export it — issue 0783).
`init` added a third and it is the largest of them: **nano-ros has no shutdown
hook at all** — not `rclcpp::on_shutdown`, not `Context::add_on_shutdown_callback`,
not the pre-shutdown variant that runs BEFORE entities are torn down. Nothing
about `no_std` prevents a fixed-capacity callback array, and a node that must
park an actuator or release a bus on the way down has nowhere to do it. That
matters more on a device than on a desktop.

`node` added the first `rename` rows, and they are the cheapest work in the
campaign: **`Node::create_subscriber` should be `create_subscription`.** rclrs,
rclcpp and rclc all say subscription, and so do our own C
(`nros_subscription_init`) and C++ (`Node::create_subscription`) — Rust is the
odd one out among our three languages as well as against ROS 2.
`subscriber_count` and `subscriber_topic_info` move with it.

`pubsub` added two more rename families, both of them one word used
consistently on each side:

* **`take` → we say `try_recv`.** rcl, rclcpp and rclrs all spell the
  non-blocking receive `take`. Both are non-blocking and both report emptiness
  without failing, so nothing asks for the other word — and `try_recv` is Rust
  channel vocabulary that reads as a different contract to a ROS 2 user.
* **SERIALIZED → we say RAW.** `publish_serialized_message`/`take_serialized`
  against `publish_raw`/`try_recv_raw`.

Plus `create_publisher`/`create_subscription` against our free-function
`make_publisher`/`make_subscription`, and `Publisher::borrow_loaned_message`
against `Publisher::loan` — the C loan API has a real shape reason (a token over
a byte range, because C has no templates and the wrapper would need an allocator
to own), but the C++ one returns a typed RAII handle on both sides, so only the
verb differs there.

Also `Node::now`
(the accessor a ported rclcpp publisher uses to stamp a header, tied to the
`Clock` gap), `node_get_domain_id`, `node_get_fully_qualified_name`,
`node_resolve_name` and `node_is_valid`. Expected shape: `create_wall_timer` as a name
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

## Blocking decisions (not an agent's to make)

Two, and they change what later stages conclude rather than how they work, so
W2 does not wait on either:

1. **Which rclrs do we mirror** — RFC-0036 says 0.7.0, the recorded surface is
   0.5.1, and they differ in the `Node = Arc<NodeState>` split. W5.
2. **Does the C `handle-owns-node` shape stay** — it is currently a signature
   rule covering six `*_fini` entry points, which asserts it is a platform
   decision. If it is not, those six become signature changes. W4.

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
