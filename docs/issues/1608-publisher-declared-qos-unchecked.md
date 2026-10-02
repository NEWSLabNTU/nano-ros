---
id: 1608
title: "A PUBLISHER's declared QoS is delivered to the build and compared with nothing — the declared-QoS check covers subscriptions only"
status: open
type: enhancement
area: [build, core]
severity: low
found: 2026-10-01
related: [1256, 1564, rfc-0100, phase-454]
---

## What

A contract can state `qos:` for a publisher endpoint (`pub_endpoints.<ep>.qos`)
and every road carries it: phase-454 W2 made a publisher's depth travel, W3
the three policies, and the sizing descriptor states all four per
`[[endpoint]]` row. `transient_local` on a publisher is the one QoS fact that
is publisher-side by nature -- it is what a zenoh image spends a queryable slot
on (`nros_sizing_descriptor::transient_local_publishers_over`).

The code/contract AGREEMENT check covers SUBSCRIPTIONS only, in all three
languages, after issue 1256 widened it from the depth to reliability and
durability:

| surface | what it checks | publishers |
| --- | --- | --- |
| `nros_declared_qos_generated.h` (C/C++ compile time) | rows keyed `(type, topic)` | none -- a publisher and a subscription on one pair would be two rows with one key, so the table has no publisher row (`EntityInventory::declared_qos_header_table`) |
| `Node::check_declared_qos` (C++ boot) | `create_subscription_in*` | no publisher counterpart |
| `nros_node::declared_qos::{check,honour}` (Rust / FFI registration) | the subscription registration seams | `build.rs` reads `EndpointKind::Subscription` rows only |

So a publisher whose code passes `volatile` against a contract that declares
`transient_local` (late joiners get nothing, and the image was sized for a
queryable it never declares) -- or the reverse (a queryable slot the build did
not count) -- builds and boots silently.

## Why it was not done with 1256

1256's open bullet was that the EXISTING header and boot check cover
reliability and durability; both were subscription-only, for depth as well.
Adding publishers is a new surface, not a wider column: the table needs a KIND
column (and `NROS_ASSERT_DECLARED_*` a publisher spelling), C++ has no
`NROS_PUBLISH` macro seam to assert at -- `create_publisher_in` is called
directly -- and the Rust publisher registration paths have no `honour` funnel
yet.

## Acceptance

A publisher whose code and contract disagree on reliability or durability
fails the build (C/C++, where the QoS is a constant expression) or the
registration (Rust, and every non-constant C/C++ call site), with a negative
control in `just check declared-qos-header` and `just check
declared-qos-registration` the way the subscription side has one.

## Update (2026-10-03) -- the RUNTIME half landed; the compile-time half is open

**What landed.** The sizing descriptor's PUBLISHER rows now reach
`nros_node::config::DECLARED_PUBLISHER_QOS_ROWS` (descriptor only -- the
`NROS_ENTITY_DECLARED_DEPTHS` env carrier is subscription depths by definition
and no knob was added), and every publisher registration goes through
`nros_node::declared_qos::honour_publisher`: the Rust seams
(`create_publisher_with_qos`, `create_publisher_raw_with_qos`, and
`Executor::create_raw_publisher_handle_on`, which every `NodeCtx` publisher
reaches) and the C/C++ FFI seams (`nros_publisher_init_with_qos`,
`nros_cpp_publisher_create`). The call is placed BEFORE QoS validation, so a
policy the contract raises a publisher to is one the backend is asked about.

**The rule, and why it is not the subscription's.** Depth is shared (take a
shallower declaration, refuse a deeper one). The two POLICIES are RxO-matched,
and the tolerant direction is opposite for a writer: a publisher that asks LESS
than its declaration is RAISED to it (the build reserved it, and every reader
that matched the lower offer still matches); one that asks MORE is REFUSED --
lowering it could break a match, and keeping it rides storage the build did not
count (the TL writer's queryable slot and retained samples, XRCE's reliable
stream). The FFI seams HONOUR rather than strict-check, because no C/C++
call-site seam takes a publisher's declaration the way `NROS_SUBSCRIBE` does.

**Measured, `examples/native/rust/talker` (zenoh, native), private router:**

* contract `/chatter` `keep_last 1` + `transient_local`: the default publisher
  registers depth 1, transient_local (`... :1:,1: ...` in its liveliness QoS),
  with both takes reported at WARN, and publishes as before;
* contract `/chatter` `reliability: best_effort`: registration refused,
  `NodeError::DeclaredQosMismatch`, the image exits naming the topic;
* no contract: `honour_publisher` folds away -- no `declared_qos` symbol in the
  binary, and removing the call leaves `create_raw_publisher_handle_on` the same
  size (the +32 B of `.text` against `origin/main` is code-generation-unit
  noise from the other edits, measured by that removal).

Tests: `declared_qos::tests::a_publisher_*` (three, one of which runs the same
row through the subscription rule and gets the opposite verdict; mutating the
publisher arm to the subscription's direction reds two), and the
`declared-qos-registration` lane's descriptor step now expects FOUR
declared-image tests -- the fourth compiled only where a publisher row arrived
(negative control: filtering the descriptor back to subscriptions reds it).

**Still open -- the compile-time half (C and C++).** The generated
`nros_declared_qos_generated.h` table carries no publisher rows. The smallest
shape that does not move the existing row arity: a SECOND X-macro list
(`NROS_DECLARED_PUB_QOS_ROWS` / `_Q`) beside the subscription one, written by
the same loop in `EntityInventory::to_declared_qos_header`, plus
`NROS_ASSERT_DECLARED_PUB_{DEPTH,RELIABILITY,DURABILITY}` in
`nros/declared_qos.h`. C++ additionally needs a call-site seam to assert at --
`create_publisher_in` is a plain call with no macro form -- which is an API
decision, not a mechanical extension. Until then a constant-expression C/C++
publisher that disagrees is refused at REGISTRATION (boot), not at build.

## Design note (2026-10-03, RFC-0100 Amendment 1) — what the build path changes

Nothing in the fix shape above; two constraints on WHERE it can be shown to
work, both from RFC-0065 D8 (one cmake configure per coordinate, so several
images share one runtime):

* **The Rust half has no table on an N:1 cmake configure today.** The declared
  policies reach Rust only on the sizing descriptor (`nros-node/build.rs`,
  `declared_qos_rows`), and a configure with several entries names no
  descriptor to cargo (issue 1649) — so there the Rust registration checks the
  subscription DEPTH (from `NROS_ENTITY_DECLARED_DEPTHS`) and no policy at all,
  and a publisher row added here would be equally absent. Demonstrate the Rust
  half on a cargo image or a single-entry configure; it reaches the
  `examples/workspaces/cpp` native configure when RFC-0100 D12 lands (one
  runtime descriptor per configure), with no change to this issue's code. Do
  NOT add an env carrier for the policies to cover that road — D10 forbids it,
  and D12 is the road's fix.
* **The C/C++ table's KIND column must be folded by the SAME per-component
  union** issue 1564 uses for subscriptions (and D12 rule 1 uses for the
  descriptor): a publisher row on which two entries' models disagree is
  REFUSED, naming both models — never resolved to one of them.

(Another agent holds this issue's code; this note changes no direction it is
following, only where the acceptance can be measured.)
