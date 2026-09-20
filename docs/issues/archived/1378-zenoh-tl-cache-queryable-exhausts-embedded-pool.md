---
id: 1378
title: "on an EMBEDDED zenoh image the action `/status` publisher's
  transient-local CACHE QUERYABLE cannot be declared, so
  `nros_executor_add_action_server` returns -1 and the server never starts"
status: resolved
type: bug
area: rmw, zephyr, nuttx
severity: high
related: [issue-1341, issue-1361, issue-0460, issue-1393, issue-1025, phase-392, phase-455]
resolved_in: "issue 1378 -- NROS_DECLARED_TL_PUBLISHERS, the declared road's half of phase-455 W5's rule"
---

## Why this exists as its own issue

Issue 1341 (the zenoh shim REFUSING `TRANSIENT_LOCAL` on the action `/status`
publisher) is fixed and archived, and issue 1361 (the terminal status sample
never being published) is fixed and archived. This is the finding that was
recorded inside 1341 and outlives it: the publisher is now SERVED, and the
cache queryable it therefore declares is what an embedded image cannot provide.
Archiving 1341 without moving this out would have buried it.

## Symptom, as 1341 recorded it (2026-09-15, not re-measured since)

Nightly **34940586021** (schedule, 2026-09-15T07:13), job **104288439509**
(`nuttx`), cells `test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
and `…lang_3_Lang__Cpp`:

```
nuttx E2E failed — readiness pattern 'Waiting for action goals' not observed.
```

and the server's own transcript says why it never got there:

```
[ERROR] nros: [0.054000] qos: publisher
  '0/fibonacci/_action/status/action_msgs::msg::dds_::GoalStatusArray_/TypeHashNotSupported'
  asked for TRANSIENT_LOCAL and its cache queryable could not be declared (Full…
[nros] examples/qemu-armv7a-nuttx/c/action-server/src/main.c:236
  nros_executor_add_action_server(&app.executor, &app.action_server) -> -1
```

**The asymmetry is measured, not inferred.** In that same job the RUST action
cell passes: `9 tests run: 6 passed, 3 failed` — the three reds are action C,
action C++ and the pubsub C cell that issue 1363 covers. Rust action, and all
three service cells, are green.

## What this points at, and what is not yet measured

A transient-local publisher's cache is a queryable under an `@adv/pub` suffix
(1341 read that off a live router), and on an embedded image the queryable table
is a small static array — `ZPICO_MAX_QUERYABLES` is 8, against which
`[param_services]` (6) and `[lifecycle]` (5) already claim eleven slots before
the app declares anything (issue 0460). That makes pool exhaustion the obvious
reading of `(Full…`, and the next thing to MEASURE rather than a conclusion:
nobody has yet counted the declared queryables in that image.

## THE COUNT, measured 2026-09-20

The pool IS the cause, and the table was not 8. It was **3**, for an image that
declares **4**.

`nros ws entity-facts --leaf` is what the CMake road asks, and it answered, for
both failing leaves:

```
--- examples/qemu-armv7a-nuttx/c/action-server
    NROS_DECLARED_INFRA_QUERYABLES=none
    NROS_DECLARED_NODES=1
    NROS_DECLARED_SERVICE_SERVERS=3
--- examples/qemu-armv7a-nuttx/cpp/action-server
    NROS_DECLARED_INFRA_QUERYABLES=none
    NROS_DECLARED_NODES=1
    NROS_DECLARED_SERVICE_SERVERS=3
```

`nros-zpico-build::queryable_default_from` turns that into `3 + 0 + 0`, so the
image gets three slots. What it declares at boot is four:

| # | queryable | who declares it |
| --- | --- | --- |
| 1 | `/fibonacci/_action/send_goal` | `ACTION_SERVER_QUERYABLES` |
| 2 | `/fibonacci/_action/cancel_goal` | `ACTION_SERVER_QUERYABLES` |
| 3 | `/fibonacci/_action/get_result` | `ACTION_SERVER_QUERYABLES` |
| 4 | `…/_action/status/…/@adv/pub/<zid>/<eid>/_` | the `/status` publisher's TL cache |

`[param_services]` and `[lifecycle]` contribute **nothing here** — the leaf
declares neither, so 0460's eleven slots are not in this image at all. The
commonly-quoted "8 against 11" is a different image's arithmetic; this one fails
at 3 against 4.

## Why C and C++ differ from Rust — and it is NOT the language

The two failing cells are not "the C ones". They are **the only two leaves in
the tree that DECLARE their entities**:

```
examples/qemu-armv7a-nuttx/c/action-server         entities=1
examples/qemu-armv7a-nuttx/cpp/action-server       entities=1
examples/qemu-armv7a-nuttx/rust/action-server      entities=0
examples/{native,zephyr,threadx-linux,rv-virt-threadx,mps2-an385-freertos}/*/action-server
                                                   entities=0   (16 leaves)
```

An undeclared leaf falls to `UNDECLARED_HEADROOM` — eight slots on an embedded
target — and boots on the headroom with four to spare. A leaf that describes
itself gets an EXACT pool, and the exact pool was short by one. **The images
that did the right thing are the ones that failed**, which is why this reads as
a C/C++ fault and is not one: `nros ws entity-facts --leaf` answers the same
three numbers whatever language the leaf is written in.

The Rust road escapes for a second, independent reason. `nros sync` writes a
single-package cargo leaf a SIZING DESCRIPTOR, and phase-455 W5 already taught
`nros-zpico-build` to add `nros_sizing_descriptor::transient_local_publishers`
to the table. `examples/native/rust/action-server/build/native/nros-cargo.toml`
carries both:

```
NROS_DECLARED_SERVICE_SERVERS = "3"
NROS_SIZING_DESCRIPTOR = { value = "nros/sizing/native.toml", relative = true }
```

and that descriptor's `[[endpoint]] kind = "action_server"` row is the fourth
slot. **A cmake / Zephyr west / NuttX entry has no descriptor at all** — issue
1393 is the standing record of why — so on those roads the term was structurally
unreachable. One rule, live on one road of three.

## The fix

**Not** a bigger ceiling, and not a conditional cache queryable:

* *Raise the default.* The table sizes `ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES`
  service buffers at **4,504 bytes a slot** (`nros-zpico-build::runner`), and
  phase-392 W5.d removed exactly this guess — it cost a native talker 144,128
  bytes for services it does not have. Padding a number that is supposed to be
  exact also hides the next off-by-one instead of failing on it.
* *Make the cache queryable conditional.* That un-does 1341: TRANSIENT_LOCAL
  stops being served, which is the whole of what phase-455 W5 bought.
* *Share one cache across a node's TL publishers.* The wire forbids it. A stock
  `ze_advanced_subscriber` queries `<topic keyexpr>/@adv/pub/**`, so the
  queryable lives UNDER the topic's own key; one queryable cannot answer for two
  topics.
* *Refuse at build time.* `check_queryable_override` already does this and could
  not fire: nobody SET the knob here. The DERIVED default was itself the short
  number, so a refusal would refuse every declared action image.

So: **count it**, on the road that could not. `NROS_DECLARED_TL_PUBLISHERS` is
that carrier, and the number it carries comes from the rule that already existed
— `nros_sizing_descriptor::transient_local_publishers_over`, which
`transient_local_publishers(desc)` is now a four-line adapter over. The CLI feeds
it declared ENTITIES instead of descriptor endpoints; nothing restates "an action
server has a transient-local `/status`" a second time (issue 1025's rule).

Three producers, because the class has three roads and fixing one is how this
tree's bugs recur:

| road | producer | what changed |
| --- | --- | --- |
| standalone leaf (the failing one) | `entity_facts::facts_from_leaf` | emits `NROS_DECLARED_TL_PUBLISHERS` |
| workspace / bringup model | `entity_facts::facts_from_model` | emits it from `declared_action_servers` |
| Zephyr / cmake derived knob | `entity_inventory::derive` | `max_queryables` gains the term |

The model road can state the action-server half only, and that is structural
rather than an omission: `ros_launch_manifest_model::TopicWiring` carries
`type` / `pub` / `sub` and **no QoS at all**, so "is this publisher
transient-local?" has no field to read there. It reaches the CLI as an
`EntityDecl::durability` through a contract sidecar, which is the road the other
two producers use. Every transient-local publisher in the tree today is an action
server, so the number is exact for every current image and a lower bound for a
hand-written TRANSIENT_LOCAL topic publisher declared only in a launch model —
closing that is issue 1393's model-only descriptor producer.

Measured after the fix, same command, same leaves:

```
--- examples/qemu-armv7a-nuttx/c/action-server
    NROS_DECLARED_INFRA_QUERYABLES=none
    NROS_DECLARED_NODES=1
    NROS_DECLARED_SERVICE_SERVERS=3
    NROS_DECLARED_TL_PUBLISHERS=1
--- examples/qemu-armv7a-nuttx/cpp/action-server
    NROS_DECLARED_INFRA_QUERYABLES=none
    NROS_DECLARED_NODES=1
    NROS_DECLARED_SERVICE_SERVERS=3
    NROS_DECLARED_TL_PUBLISHERS=1
```

`refused` travels as a WORD rather than being dropped, so a composer that LOOKED
and could not answer makes the consumer say so instead of contributing a zero
indistinguishable from a measured one.

## The budget, since this is a static array

One queryable slot is **4,504 bytes** of `.bss` — `SERVICE_BUFFERS` is
`ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES` service buffers, the figure
phase-392 W5.d measured when it deleted the `if hosted { 32 } else { 8 }` guess
(a native talker was paying 144,128 B for services it does not have).

So the three candidates cost, on the NuttX C action-server image:

| | slots | Δ `.bss` | buys |
| --- | --- | --- | --- |
| today | 3 | — | an image that does not start |
| **count the TL publisher** | **4** | **+4,504 B** | the image starts |
| raise the embedded default to 8 | 8 | +22,520 B | four slots nobody measured, and the next off-by-one hidden rather than failed |

The fix is the cheapest of the three AND the only one that stays exact: it adds
a slot per transient-local publisher the image actually declares, so an image
with no action server pays nothing. A ceiling raise is five times the cost, is
wrong in both directions for the next image, and re-creates the guess phase-392
W5.d removed.

## The image starts — measured 2026-09-21

`examples/qemu-armv7a-nuttx/c/action-server`, built by
`just nuttx build-fixtures-arm` at 00:17 and run on QEMU `virt`/cortex-a7
against an `rmw_zenohd` on `tcp/0.0.0.0:8320`. The carrier reaches the build:
the leaf's own `build-zenoh/build.ninja` now names
`NROS_DECLARED_TL_PUBLISHERS=1` beside `NROS_DECLARED_SERVICE_SERVERS=3`.

```
image:  examples/qemu-armv7a-nuttx/c/action-server/build-zenoh/nros-nuttx-ffi-out/nros-nuttx-ffi
built:  2026-09-21 00:17:06.096154425 +0800
router: /opt/ros/humble/lib/rmw_zenoh_cpp/rmw_zenohd
--- image output ---
nros C Action Server (Fibonacci)
===================================
Locator: tcp/10.0.2.2:8320
Domain ID: 0
Support initialized
Node created: fibonacci_action_server
Action server created: /fibonacci
[WARN] nros: [    0.050000] qos: service '/fibonacci/_action/send_goal' asked for KEEP_LAST(10); this image's receive ring holds 4. Granting 4 and advertising it to the graph. Raise ZPICO_SUBSCRIBER_RING_DEPTH to keep more.

Waiting for action goals (Ctrl+C to exit)...

[INFO] nros: [    0.059000] arena over-provisioned: set NROS_EXECUTOR_ARENA_SIZE=7168 (Zephyr: CONFIG_ prefix). 6344/74240 bytes claimed at first spin; later registrations need more. issue 0900
```

`Action server created: /fibonacci` and `Waiting for action goals` are the two
lines the nightly never reached — its readiness pattern is that second one, and
its transcript stopped at `nros_executor_add_action_server(…) -> -1` instead.
No `TRANSIENT_LOCAL … could not be declared` anywhere in the run.

The two remaining lines are pre-existing and unrelated: a `KEEP_LAST(10)`
request granted 4 by the receive ring, and issue 0900's arena advisory.

The C++ cell, the other red in that job, on its own locator (`…:8420`):

```
nros C++ Action Server (Fibonacci)
===================================
Node created: fibonacci_action_server
[WARN] nros: [    0.049000] qos: service '/fibonacci/_action/send_goal' asked for KEEP_LAST(10); this image's receive ring holds 4. Granting 4 and advertising it to the graph. Raise ZPICO_SUBSCRIBER_RING_DEPTH to keep more.

Waiting for action goals (Ctrl+C to exit)...
```

## What is NOT verified

* **The full `just nuttx build-fixtures-arm` lane is still red**, for a reason
  that is not this: `examples/workspaces/rust` fails `cargo metadata` on a
  workspace member `src/esp32_entry` whose manifest does not exist. Both action
  images were built and run before that step.
* No `test_rtos_action_e2e` cell was run — the two images were booted by hand
  against an `rmw_zenohd`, not through the harness.
* The Zephyr and workspace roads take the same fix through
  `entity_inventory::derive`, and that arm is covered by a unit test rather than
  by a built image: no in-tree Zephyr leaf declares `nano_ros_node_register(...
  ENTITIES ...)` for an action server today, so the derivation refuses there and
  the image keeps its 8-slot budget either way.

## A second, smaller defect in the same line

The message is cut mid-word at `(Full…`, so whatever the shim's reason code says
after it never reaches the log. A diagnostic that truncates exactly where the
cause is named costs the reader the one fact they came for.

**Cause, and the fix for the whole family.** `nros_log`'s call-site buffer is
256 bytes and truncates with `…` (`nros-log/src/buffer.rs`,
`buffer-size-256` by default). The line opened with the topic key, and a ROS
action status key is ~90 of those bytes
(`0/fibonacci/_action/status/action_msgs::msg::dds_::GoalStatusArray_/TypeHashNotSupported`),
so the message spent its budget before saying anything. Raising the buffer would
cost every image RAM to fix one sentence. The three `declare_retention` messages
now put **the cause and the knob first and the topic key last**, so the part that
may truncate is the part the reader already knows.

## What would close it

1. Count the declared queryables in the failing NuttX C image and say whether
   the pool is the cause — a count, not an inference.
2. If it is: the derivation that sizes `ZPICO_MAX_QUERYABLES` must count a
   transient-local publisher's cache queryable on the embedded path too. It
   already does on native (1341's own fix derived 4), which is consistent with
   the Rust cell passing and the C/C++ ones failing, and is the first thing to
   check.
3. Untruncate the diagnostic, whatever the answer to 1 and 2 is.
