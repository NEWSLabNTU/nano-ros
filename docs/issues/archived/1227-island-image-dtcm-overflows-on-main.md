---
id: 1227
title: "The safety-island Zephyr image no longer links on main -- DTCM
  overflowed by 45040 bytes"
status: resolved
type: bug
area: zephyr
severity: high
resolved_in: "phase-403 step 2 wiring"
related: [issue-1197, issue-1132, issue-1190, issue-1015]
resolved: 2026-09-09
---

## Symptom

Autoware Safety Island's `board-build` recipe (that repo's justfile, not
this one) for `mr_canhubk3/s32k344` fails at the link:

```
ld.bfd: region `DTCM' overflowed by 45040 bytes
collect2: error: ld returned 1 exit status
FAILED: zephyr/zephyr_pre0.elf zephyr/zephyr_pre0.map
```

Reproduced on a wiped build directory, twice, with the SAME byte count both
times -- so this is not the derived-knob convergence that needs a second pass.
The derived knobs are identical to a build that links:

```
NROS_EXECUTOR_MAX_CBS=19
NROS_SUBSCRIPTION_BUFFER_SIZE=1496
NROS_SUBSCRIBER_BUFFER_SIZE=880
entity inventory: 4 components -- 33 entities, 19 executor callback slots
```

## Scale

DTCM is 128 KB. A build of this image that links reports:

```
DTCM:       93344 B       128 KB     71.22%
```

Overflowing by 45040 puts it near 176 KB, so roughly 83 KB has appeared. That is
not drift; something is placed in DTCM that was not there before, or is placed
twice.

## Bisect: four verdicts, and why it could not be finished

Measured on `mr_canhubk3/s32k344`, wiped build directory each time. BAD is
always the SAME byte count, which is itself evidence it is one cause:

| commit | verdict |
| --- | --- |
| `1976727d8` | GOOD -- DTCM 93344 B, 71.22% |
| `e3786749a` | GOOD -- DTCM 93344 B, 71.22% |
| `d7e7bb477` | GOOD -- DTCM 93336 B, 71.21% |
| `fb94d1896` | BAD -- overflowed by 45040 bytes |

`d7e7bb477` (2026-09-06 21:14) is the newest commit known to build, and is the
floor to search from. The regression is in what `main` gained between it and
`fb94d1896`.

NOT environmental: `e3786749a` reproduces 71.22% a day after the same tree first
produced it, and the three GOOD commits agree to within 8 bytes.

### Two wrong readings, recorded so they are not repeated

* **"the first suspect is `dc2416f6e`"** -- from reading subject lines
  (`the placement arithmetic moves to a crate a build script can call`). It IS
  bad, but so is every commit below it in that range, including `3e5dc6e4f`,
  a CI cache fix that cannot move RAM. Reading a subject line is not a bisect.
* **`01cb87d8f` reported BAD -- RETRACTED.** It built only because it ran before
  the probe directory was touched, so `sync` skipped metadata regeneration and
  it consumed metadata emitted by ANOTHER commit's codegen. Its number is not
  attributable to that commit. Any narrowing that rested on it (an 11-commit
  range was claimed) is withdrawn.

### What blocks finishing it

Most commits in the range cannot build the CURRENT safety-island tree at all.
The CLI's `sync` step regenerates the message metadata whenever the CLI
changes, and at those commits regeneration fails:

```
build/nros-metadata/metadata-probe-cmake/build/nros-ws-nav_msgs/.../nav_msgs_msg_odometry.hpp:99:93:
error: static assertion failed: NROS_UNBOUNDED__nav_msgs_msg_odometry__field_header_frame_id:
nav_msgs/Odometry states no serialized-size bound
```

Reproduced at `0368d4040`, `5a3cbc008`, `3da478ffb`, `4d6e0da9c`, `09ed8e069`,
`c89431210`, `29ea7d181`, `e957bc634` -- with and without clearing
`build/nros-metadata`. ASI's caps are not being applied by those commits'
codegen, so the consumer's own messages come out unbounded.

Bisecting a library against a fixed consumer assumes the consumer builds at
every commit. Here it does not, and the commits that do not build are most of
the interval. Finishing this needs the ASI side moved in lockstep with the
library, or that codegen incompatibility understood first -- not more builds of
the same shape.

### Also in the way

Issue 1235: the `sync` step refuses a CORRECTLY paired resolver on these commits, and
`setup-cli` / `setup-launch-resolve` cannot repair what their own errors name.
Every bisect step needs this working around before it can even reach a compiler.

## ROOT CAUSE FOUND -- `0368d4040`, and it is a correct fix exposing a real gap

Bisected on the HOST, seconds per step instead of twelve minutes. `nros-node`'s
`build.rs` derives the arena from the knobs and prints `cargo:arena_size`, so
the island's number is reproducible with a plain `cargo build -p nros-node` and
the island's knobs in the environment -- no Zephyr, no board, and every commit
in the interval builds, which is what made it searchable at all after the board
route stalled.

| commit | derived arena |
| --- | --- |
| `316915dc3` (parent) | 61,936 |
| **`0368d4040`** | **207,096** |
| `6e7779b05` (main today) | 207,096 |

`git bisect run` over `d7e7bb477..fb94d1896` names `0368d4040` --
`fix(#1190): the arena priced a subscription's QoS history at three slots`. The
arena grows by 145,160 B in one commit and has stayed there since.

### It is not a mistake to revert

That commit CORRECTED an under-count. The old model charged `3 * rx_buf + 512`
per subscription -- a `TripleBuffer`, the `depth <= 1` case -- while the runtime
registers `KEEP_LAST(10)`: eleven slots plus a length array. Issue 1190 is the
symptom of the old number, a zenoh listener taking `BufferTooSmall` on register.

So the island was never really fitting. The model was under-counting what the
image already allocates, and DTCM at 71.22% measured the wrong quantity.

### The gap it exposes

`PUBSUB_QOS_DEPTH` is a hardcoded `const ... = 10`
(`packages/core/nros-node/build.rs:27`). It is read neither from the environment
nor from the image's DECLARED QoS, so every subscription is priced at the ROS
default whatever the image says.

The island declares no depths at all -- `safety_island.contract.yaml` contains
zero `depth:` keys -- so today it correctly inherits KEEP_LAST(10) and correctly
pays for it. But the machinery to know better already exists: the declared-QoS
producer (issue 1084) emits `nros_declared_qos_generated.h` and
`check-declared-qos-header` gates it. The arena model is the one consumer that
does not read it.

Two separable pieces of work:

1. **`nros-node`** -- price each subscription at its DECLARED depth, falling
   back to the ROS default only where nothing is declared. A per-image constant
   of 10 makes every shallow subscription pay for a deep one.
2. **the island** -- declare the depths it actually needs. At depth 1 a
   subscription is a `TripleBuffer` again, and eleven of those cost a fraction
   of eleven-slot rings. That is a queueing decision, and it should be made
   deliberately rather than inherited.

Until one of those lands the image does not fit, and the honest reading is that
it has not fitted since `0368d4040` started telling the truth about it.

## Why this matters more than the byte count

Nothing merge-gating builds this image. The regression reached main and the only
reason it is known is that someone tried to verify an unrelated cmake fix
against a real board image. That is the same coverage hole issue 1177 is about,
one platform over.

## Resolved — the knob exists, and the report says which way to move it

phase-412 W3b landed `NROS_PUBSUB_QOS_DEPTH` (`7ff68c777`), the image-wide
override that sits at the top of the ladder

    env > Kconfig/board > declared endpoints > ROS 2 default

so an image that over-bills its arena can state the depth it actually needs.
The remaining half — and the reason this issue stayed open after the knob
existed — is that the runtime's one channel did not say which way to move it.

## It is a CEILING, and the two directions are asymmetric

This is the sharp edge, and it is not what the failing image alone suggests.
ONE number covers every subscription in the image, so an image that mixes
depths must state its **deepest**. The failing image is uniform — 11
subscription sites across 3 files, every one `::nros::QoS(1)`, verified — but
its repository is not. Surveyed 2026-09-09:

```
24 x QoS(1)     35 x QoS(10)     2 x QoS(100)     1 x QoS(0)
```

So "my subscriptions are shallow, set it to 1" is a mistake this knob actively
invites, and it fails in the quiet direction:

| stated | arena | failure | how loud |
| --- | --- | --- | --- |
| too HIGH | over-billed | a LINK error | loud — this issue, found in minutes |
| too LOW | under-billed | `NodeError::BufferTooSmall` at `Node::register` | RUNTIME, on a target where a return code may be all there is |

`report_arena_exhausted` therefore names the knob AND the word `CEILING`, so
the one channel the runtime has says which way to move. A unit test asserts
both — `an_exhausted_arena_decodes_to_the_knob_an_operator_must_set` — because
a message naming only the byte-count knob leaves the reader to rediscover the
ceiling. Mutation-checked: replacing the knob's name in the message fails that
test with the assertion above.

### Why there is no GUARD, and what one would cost

The obvious guard — refuse a stated depth below what the image's own endpoints
ask for — has nothing to read. Depth lives in each endpoint's own source
(`::nros::QoS(1)` at the call site), and the only structured view of it,
`NROS_ENTITY_DECLARED_DEPTHS`, reports **0 declared / 29 undeclared** for
exactly this image. A guard over an empty set forecloses nothing while reading
as protection, which is the trap issue 1015's first fix fell into.

Priced, for when the data exists: it is `max(declared depths) <= stated` in
`nros-node/build.rs` as a `compile_error!`, ~10 lines, once phase-403 step 2
carries `NROS_ENTITY_DECLARED_DEPTHS` and `NROS_ENTITY_UNDECLARED_DEPTH_COUNT`
into the cargo environment (they reach cmake and stop). It must refuse only
against DECLARED depths and stay silent while any endpoint is undeclared —
guarding on a partial set would reject correct images. That wiring is the
prerequisite, not this issue.

Superseded in part by the section below: the forwarding landed, and
`nros-node/build.rs` now sums the DECLARED depths and refuses to size from
depth at all unless the subscription-scoped undeclared count is present and
zero. What is still absent is the other half — a `compile_error!` when a
STATED `NROS_PUBSUB_QOS_DEPTH` sits below `max(declared depths)`. The survey
above is why that half still matters: the ceiling's low direction fails at
runtime, and the data to refuse it now exists.

## What the search left behind

The board-build route to bisecting this is still broken and worth fixing on its
own account -- most commits in the interval cannot build the current ASI tree,
and issue 1235 blocks the rest -- but it is no longer on the path to an answer
here. The host oracle (`cargo:arena_size`) should be the first tool reached for
any future memory-derivation bisect: three orders of magnitude cheaper, and it
does not depend on the consumer building at all.

## RESOLVED -- the declared depths now reach the model

The chain phase-403 step 2 left half-built is finished, and the image links:

    DTCM: 98,040 B    128 KB    74.80%

Four changes, no revert of `0368d4040` and no extra RAM granted:

* the vendored `play_launch` pin moves `db4af878` -> `0647131a`. Upstream had
  already implemented the endpoint QoS merge (`effective_qos(topic_qos,
  p.qos)`); the vendored copy was 30 commits behind and still carried
  `qos: None` hardcoded in `sub_contract`. `ros-launch-manifest` needed nothing
  -- the pinned `v0.1.31` already had `qos: Option<QosDecl>` with
  `depth: Option<u32>` on endpoints.
* the inventory publishes `NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION`.
  The broad count admits publishers and services -- `carries_qos_depth` says of
  a publisher that its depth "sizes no receive buffer, so nothing reads it yet"
  -- so it stands at 18 on this image while all eleven subscriptions declare.
  A consumer that prices only the subscription term cannot refuse on it.
* `nros_cargo_build.cmake` forwards the depth knobs, converting `;` to `,`
  first. `;` is cmake's LIST separator and the cargo env is a flat string, so
  the raw value delivered ONE triple of eleven: every knob looked present, the
  lane silently kept the worst case, and the image overflowed by exactly the
  same 45,376 bytes it had before the wiring existed.
* `nros-node/build.rs` sums the declared depths per endpoint, and refuses to
  size from depth at all unless the subscription-scoped undeclared count is
  present and zero.

The island states `qos: { depth: 1 }` on its eleven `sub:` endpoints, which is
what its code has always passed to `create_subscription`. The boot-time check
(`check_declared_depth`) requires the two to agree, so the declaration is
verified against the call sites rather than trusted.

`PUBSUB_QOS_DEPTH` remains as the fallback for an image that declares nothing,
which is the right value for that case: it is what a normally-written ROS 2 node
registers (`rmw_qos_profile_default`, KEEP_LAST(10)). Moving that literal into a
config rung is left open -- see the note in the root-cause section.
